use crate::ides::IdeVersion;
use anyhow::anyhow;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use futures::stream::iter;
use futures::{StreamExt, TryStreamExt};
use lazy_static::lazy_static;
use log::{debug, info, warn};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::exists;
use std::future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::{read_to_string, write};
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tokio_retry2::strategy::ExponentialBackoff;
use tokio_retry2::{Retry, RetryError};
use version_compare::Version;
use which::which;

const ALL_PLUGINS_JSON: &str = "all_plugins.json";

lazy_static! {
    static ref NIX_PREFETCH_URL: PathBuf =
        which("nix-prefetch-url").expect("nix-prefetch-url not in PATH");
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialOrd, PartialEq, Ord, Eq, Hash)]
pub struct PluginVersion(String);

impl PluginVersion {
    const SEPARATOR: &'static str = "/--/";
    pub fn new(name: &str, version: &str) -> Self {
        Self(format!("{}{}{}", name, Self::SEPARATOR, version))
    }
}

type PluginCache = HashMap<PluginVersion, PluginDbEntry>;
// Plugins for which download requests have 404ed
type FourOFourCache = HashSet<PluginVersion>;

pub struct PluginDb {
    // all_plugins caches all entries, ides contains references to them.
    all_plugins: BTreeMap<PluginVersion, &'static PluginDbEntry>,
    ides: HashMap<IdeVersion, BTreeMap<String, String>>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct PluginDetails {
    category: Option<PluginDetailsCategory>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct PluginDetailsCategory {
    #[serde(rename = "idea-plugin")]
    idea_plugin: Vec<PluginDetailsIdeaPlugin>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct PluginDetailsIdeaPlugin {
    version: String,
    #[serde(rename = "idea-version")]
    idea_version: PluginDetailsIdeaVersion,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct PluginDetailsIdeaVersion {
    #[serde(rename = "since-build")]
    since_build: Option<String>,
    #[serde(rename = "until-build")]
    until_build: Option<String>,
}

impl PluginDb {
    pub fn new() -> Self {
        Self {
            all_plugins: Default::default(),
            ides: Default::default(),
        }
    }
    pub fn insert(
        &mut self,
        ideversion: &IdeVersion,
        name: &str,
        version: &str,
        entry: &PluginDbEntry,
    ) {
        let version_entry = self.ides.entry(ideversion.clone()).or_default();
        // We leak here since self-referential structs are otherwise a nightmare and it doesn't
        // really matter in this CLI app.
        self.all_plugins
            .entry(PluginVersion::new(name, version))
            .or_insert_with(|| Box::leak(Box::new(entry.clone())));
        version_entry.insert(name.to_string(), version.to_string());
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct PluginDbEntry {
    #[serde(rename = "p")]
    pub path: String,
    #[serde(rename = "h")]
    pub hash: String,
}

pub async fn index(url: &str) -> anyhow::Result<Vec<String>> {
    Ok(reqwest::get(url).await?.json().await?)
}

pub async fn db_cache_load(out_dir: &Path) -> anyhow::Result<PluginCache> {
    let file = out_dir.join(ALL_PLUGINS_JSON);
    if exists(&file)? {
        Ok(serde_json::from_str(&read_to_string(file).await?)?)
    } else {
        Ok(PluginCache::new())
    }
}

pub async fn build_db(
    cache: &PluginCache,
    ides: &[IdeVersion],
    pluginkeys: &[String],
) -> anyhow::Result<PluginDb> {
    let cache = Arc::new(cache);
    let client = Arc::new(
        Client::builder()
            .timeout(Duration::from_secs(600))
            .build()?,
    );
    let fof_cache = Arc::new(RwLock::new(FourOFourCache::new()));
    let db = Arc::new(RwLock::new(PluginDb::new()));

    let mut futures = Vec::new();

    for pluginkey in pluginkeys {
        let fof_cache = fof_cache.clone();
        let db = db.clone();
        let client = client.clone();
        let cache = cache.clone();

        // Create a future that will be retried 3 times, has a timeout of 1200 seconds per try
        // and polls process_plugin to process this plugin for this IDE version. process_plugin
        // will update the database.
        futures.push(async move {
            Retry::spawn(ExponentialBackoff::from_millis(250).take(3), move || {
                let fof_cache = fof_cache.clone();
                let db = db.clone();
                let client = client.clone();
                let cache = cache.clone();
                async move {
                    let res = timeout(
                        Duration::from_secs(1200),
                        process_plugin(
                            db.clone(),
                            client.clone(),
                            ides,
                            pluginkey,
                            &cache,
                            fof_cache.clone(),
                        ),
                    )
                    .await;
                    match res {
                        Ok(Ok(v)) => Ok(v),
                        Ok(Err(e)) => {
                            warn!("failed plugin processing {pluginkey}: {e}. Might retry.");
                            Err(RetryError::transient(e))
                        }
                        Err(e) => {
                            warn!(
                                "failed plugin processing {pluginkey} due to timeout. Might retry."
                            );
                            Err(RetryError::transient(anyhow!("timeout").context(e)))
                        }
                    }
                }
            })
            .await
        });
    }

    iter(futures)
        .buffered(16)
        // TODO: try_collect does not exit early. try_all does. Is there any better way to do this?
        .try_all(|()| future::ready(true))
        .await?;

    Ok(Arc::into_inner(db).unwrap().into_inner())
}

/// Various hacks to support (or skip) some very odd cases
fn hacks_for_details_key(pluginkey: &str) -> Option<&str> {
    match pluginkey {
        // The former is the real ID, but it trips up the plugin endpoint...
        "23.bytecode-disassembler" => Some("bytecode-disassembler"),
        // Has invalid version numbers
        "com.valord577.mybatis-navigator" => None,
        // ZIP contains invalid file names
        "io.github.kings1990.FastRequest" => None,
        v => Some(v),
    }
}

async fn process_plugin(
    db: Arc<RwLock<PluginDb>>,
    client: Arc<Client>,
    ides: &[IdeVersion],
    pluginkey: &str,
    cache: &PluginCache,
    fof_cache: Arc<RwLock<FourOFourCache>>,
) -> anyhow::Result<()> {
    debug!("Processing {pluginkey}...");

    let Some(pluginkey_for_details) = hacks_for_details_key(pluginkey) else {
        warn!("{pluginkey}: plugin is marked as broken, skipping...");
        return Ok(());
    };

    let req = client
        .get(format!(
            "https://plugins.jetbrains.com/plugins/list?pluginId={}",
            pluginkey_for_details
        ))
        .send()
        .await?;
    if !req.status().is_success() {
        return Err(anyhow!(
            "{} failed details request: {}",
            pluginkey,
            req.status()
        ));
    }
    let details: PluginDetails = serde_xml_rs::from_str(&req.text().await?)?;

    let Some(category) = details.category else {
        warn!("{pluginkey}: No plugin details available. Skipping!");
        return Ok(());
    };

    let mut versions = category.idea_plugin;
    versions.sort_by(|a, b| b.version.cmp(&a.version));

    for ide in ides {
        match supported_version(ide, &versions) {
            None => warn!("{pluginkey}: IDE {ide:?} not supported."),
            Some(version) => {
                let entry =
                    get_db_entry(&client, pluginkey, &version.version, &db, cache, &fof_cache)
                        .await?;
                if let Some(entry) = entry {
                    let mut lck = db.write().await;
                    lck.insert(ide, pluginkey, &version.version, &entry);
                }
            }
        }
    }
    Ok(())
}

fn supported_version<'a>(
    ide: &IdeVersion,
    versions: &'a Vec<PluginDetailsIdeaPlugin>,
) -> Option<&'a PluginDetailsIdeaPlugin> {
    let build_version = Version::from(&ide.build_number).unwrap();
    for version in versions {
        if let Some(min) = version.idea_version.since_build.as_ref() {
            if build_version < Version::from(&min.replace(".*", ".0")).unwrap() {
                continue;
            }
        }
        if let Some(max) = version.idea_version.until_build.as_ref() {
            if build_version > Version::from(&max.replace(".*", ".99999999")).unwrap() {
                continue;
            }
        }
        return Some(version);
    }
    None
}

async fn get_db_entry<'a>(
    client: &Client,
    pluginkey: &str,
    version: &str,
    current_db: &RwLock<PluginDb>,
    cache: &'a PluginCache,
    fof_cache: &RwLock<FourOFourCache>,
) -> anyhow::Result<Option<Cow<'a, PluginDbEntry>>> {
    let key = PluginVersion::new(pluginkey, version);
    // Look in current_db
    {
        let db_lck = current_db.read().await;
        let v = db_lck.all_plugins.get(&key);
        if let Some(v) = v {
            return Ok(Some(Cow::Borrowed(v)));
        }
    };
    // Look in cache
    if let Some(v) = cache.get(&key) {
        return Ok(Some(Cow::Borrowed(v)));
    }

    {
        if fof_cache.read().await.contains(&key) {
            return Ok(None);
        }
    }

    info!(
        "{}@{}: Plugin not yet cached, downloading for hash...",
        pluginkey, version
    );

    let req = client
        .head(format!(
            "https://plugins.jetbrains.com/plugin/download?pluginId={}&version={}",
            pluginkey, version
        ))
        .send()
        .await?;

    if req.status() == StatusCode::NOT_FOUND {
        warn!("{}@{}: not available: skipping", pluginkey, version);
        fof_cache.write().await.insert(key);
        return Ok(None);
    } else if !req.status().is_success() {
        return Err(anyhow!(
            "{}@{}: failed download HEAD request: {}",
            pluginkey,
            version,
            req.status()
        ));
    }

    const PREFIX_OF_ALL_URLS: &str = "https://downloads.marketplace.jetbrains.com/";
    // Query parameters don't seem to result in different files, probably only for analytics.
    // Remove them to save some space.
    // Also remove the https://downloads.marketplace.jetbrains.com/ prefix.
    let mut url = req.url().clone();
    url.set_query(None);
    let url = url.to_string();

    let is_jar = url.ends_with(".jar");
    let hash_nix32 = get_nix32_hash(
        &format!("{pluginkey}-{version}-source").replace(|c: char| !c.is_alphanumeric(), "-"),
        &url,
        !is_jar,
        is_jar,
    )
    .await?;
    let hash = BASE64_STANDARD.encode(
        nix_base32::from_nix_base32(&hash_nix32)
            .ok_or_else(|| anyhow!("{}@{}: failed decoding nix hash", pluginkey, version,))?,
    );

    let path = url
        .strip_prefix(PREFIX_OF_ALL_URLS)
        .expect("expect all URLs to start with prefix.")
        .to_string();

    Ok(Some(Cow::Owned(PluginDbEntry { path, hash })))
}

async fn get_nix32_hash(
    name: &str,
    url: &str,
    unpack: bool,
    executable: bool,
) -> anyhow::Result<String> {
    let mut parameters = Vec::with_capacity(8);
    parameters.push("--type");
    parameters.push("sha256");
    parameters.push("--name");
    parameters.push(name);
    if unpack {
        parameters.push("--unpack");
    }
    if executable {
        parameters.push("--executable");
    }
    parameters.push(url);

    let child = Command::new(&*NIX_PREFETCH_URL)
        .args(parameters)
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let result = child.wait_with_output().await?;
    if !result.status.success() {
        return Err(anyhow!("nix-prefetch-url failed for {url}"));
    }
    let out = String::from_utf8(result.stdout)?.trim().to_string();

    Ok(out)
}

pub async fn db_save(output_folder: &Path, db: PluginDb) -> anyhow::Result<()> {
    // all plugins
    let out_path = output_folder.join(ALL_PLUGINS_JSON);
    debug!("Generating {out_path:?}...");
    write(out_path, serde_json::to_string_pretty(&db.all_plugins)?).await?;

    // mappings
    let output_folder = output_folder.join("ides");
    for (ide, plugins) in db.ides {
        let out_path = output_folder.join(format!("{}-{}.json", ide.ide.nix_key(), ide.version));
        debug!("Generating {out_path:?}...");
        write(out_path, serde_json::to_string_pretty(&plugins)?).await?;
    }
    Ok(())
}
