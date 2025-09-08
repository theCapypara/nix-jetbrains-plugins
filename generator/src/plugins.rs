use crate::ides::IdeVersion;
use anyhow::anyhow;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
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
use std::mem::take;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::{read_dir, read_to_string, write};
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tokio_retry2::strategy::ExponentialBackoff;
use tokio_retry2::{Retry, RetryError};
use tokio_stream::wrappers::ReadDirStream;
use version_compare::Version;
use which::which;

const ALL_PLUGINS_JSON: &str = "all_plugins.json";

lazy_static! {
    static ref NIX_PREFETCH_URL: PathBuf =
        which("nix-prefetch-url").expect("nix-prefetch-url not in PATH");
    static ref NIX_STORE: PathBuf = which("nix-store").expect("nix-store not in PATH");
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialOrd, PartialEq, Ord, Eq, Hash)]
pub struct PluginVersion(String);

impl PluginVersion {
    const SEPARATOR: &'static str = "/--/";
    pub fn new(name: &str, version: &str) -> Self {
        Self(format!("{}{}{}", name, Self::SEPARATOR, version))
    }
}
// Plugins for which download requests have 404ed
type FourOFourCache = HashSet<PluginVersion>;

pub struct PluginDb {
    // all_plugins caches all entries, ides contains references to them.
    all_plugins: BTreeMap<PluginVersion, &'static PluginDbEntry>,
    ides: HashMap<IdeVersion, BTreeMap<String, String>>,
}

impl PluginDb {
    pub fn new() -> Self {
        Self {
            all_plugins: Default::default(),
            ides: Default::default(),
        }
    }

    fn init(init: impl IntoIterator<Item = (PluginVersion, PluginDbEntry)>) -> PluginDb {
        Self {
            // see insert on why we do this
            all_plugins: init
                .into_iter()
                .map(|(k, v)| {
                    // see insert on why we do this
                    let v: &'static _ = Box::leak(Box::new(v));
                    (k, v)
                })
                .collect(),
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
    #[serde(rename = "@since-build")]
    since_build: Option<String>,
    #[serde(rename = "@until-build")]
    until_build: Option<String>,
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

/// Load the plugin database, all_plugins.json only!
pub async fn db_load(out_dir: &Path) -> anyhow::Result<PluginDb> {
    let file = out_dir.join(ALL_PLUGINS_JSON);
    if exists(&file)? {
        Ok(PluginDb::init(serde_json::from_str::<'_, HashMap<_, _>>(
            &read_to_string(file).await?,
        )?))
    } else {
        Ok(PluginDb::new())
    }
}

/// Load the plugin database, including the IDE mappings.
/// WARNING: Does not populate build numbers for IDEs!
pub async fn db_load_full(out_dir: &Path) -> anyhow::Result<PluginDb> {
    let mut db = db_load(out_dir).await?;
    let db_mut = Arc::new(RwLock::new(&mut db));

    ReadDirStream::new(read_dir(out_dir.join("ides")).await?)
        .and_then(|file| {
            let db_mut = db_mut.clone();
            async move {
                let Some(ideversion) =
                    IdeVersion::from_json_filename(&file.file_name().to_string_lossy())
                else {
                    warn!(
                        "Invalid JSON file in ide directory skipped: {}",
                        file.path().display()
                    );
                    return Ok(());
                };
                let ide_mapping: BTreeMap<String, String> =
                    serde_json::from_str(&read_to_string(file.path()).await?)?;
                let mut lck = db_mut.write().await;
                let db_mut = &mut *lck;
                db_mut.ides.insert(ideversion, ide_mapping);
                Ok(())
            }
        })
        .try_collect::<()>()
        .await?;

    Ok(db)
}

pub async fn db_update(
    db: &mut PluginDb,
    ides: &[IdeVersion],
    pluginkeys: &[String],
) -> anyhow::Result<()> {
    let client = Arc::new(
        Client::builder()
            .timeout(Duration::from_secs(600))
            .build()?,
    );
    let fof_cache = Arc::new(RwLock::new(FourOFourCache::new()));
    let db = Arc::new(RwLock::new(db));

    let mut futures = Vec::new();

    for pluginkey in pluginkeys {
        let fof_cache = fof_cache.clone();
        let db = db.clone();
        let client = client.clone();

        // Create a future that will be retried 3 times, has a timeout of 1200 seconds per try
        // and polls process_plugin to process this plugin for this IDE version. process_plugin
        // will update the database.
        futures.push(async move {
            Retry::spawn(ExponentialBackoff::from_millis(250).take(3), move || {
                let fof_cache = fof_cache.clone();
                let db = db.clone();
                let client = client.clone();
                async move {
                    let res = timeout(
                        Duration::from_secs(1200),
                        process_plugin(
                            db.clone(),
                            client.clone(),
                            ides,
                            pluginkey,
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

    Ok(())
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
        // ZIP contains invalid file names
        "com.majera.intellij.codereview.gitlab" => None,
        v => Some(v),
    }
}

async fn process_plugin(
    db: Arc<RwLock<&mut PluginDb>>,
    client: Arc<Client>,
    ides: &[IdeVersion],
    pluginkey: &str,
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

    let versions = category.idea_plugin;
    // TODO: This doesn't work as compare_versions's order is somehow not always total.
    //       We will rely on the order in the response being correct for now.
    //       Just naively sorting the strings is NOT correct!
    //versions.sort_by(|a, b| {
    //    Version::from(&b.version)
    //        .unwrap()
    //        .partial_cmp(&Version::from(&a.version).unwrap())
    //        .unwrap_or(Ordering::Equal)
    //});

    for ide in ides {
        match supported_version(ide, &versions) {
            None => debug!("{pluginkey}: IDE {ide:?} not supported."),
            Some(version) => {
                let entry =
                    get_db_entry(&client, pluginkey, &version.version, &db, &fof_cache).await?;
                if let Some(entry) = entry {
                    let mut lck = db.write().await;
                    let db_mut = &mut *lck;
                    db_mut.insert(ide, pluginkey, &version.version, &entry);
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
        if let Some(min) = version.idea_version.since_build.as_ref()
            && build_version < Version::from(&min.replace(".*", ".0")).unwrap() {
                continue;
            }
        if let Some(max) = version.idea_version.until_build.as_ref()
            && build_version > Version::from(&max.replace(".*", ".99999999")).unwrap() {
                continue;
            }
        return Some(version);
    }
    None
}

async fn get_db_entry<'a>(
    client: &Client,
    pluginkey: &str,
    version: &str,
    current_db: &RwLock<&mut PluginDb>,
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
    parameters.push("--print-path");
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
    let Some((hash, path)) = &out.split_once('\n') else {
        return Err(anyhow!(
            "nix-prefetch-url generated invalid output to stdout: {out}"
        ));
    };

    // We forget the store path again to save disk space
    Command::new(&*NIX_STORE)
        .args(["--delete", path])
        .stdout(Stdio::piped())
        .spawn()?;

    Ok(hash.to_string())
}

pub async fn db_save(output_folder: &Path, db: PluginDb) -> anyhow::Result<()> {
    // all plugins
    let out_path = output_folder.join(ALL_PLUGINS_JSON);
    debug!("Generating {out_path:?}...");
    write(out_path, serde_json::to_string_pretty(&db.all_plugins)?).await?;

    // mappings
    let output_folder = output_folder.join("ides");
    for (ide, plugins) in db.ides {
        let out_path = output_folder.join(ide.to_json_filename());
        debug!("Generating {out_path:?}...");
        write(out_path, serde_json::to_string_pretty(&plugins)?).await?;
    }
    Ok(())
}

pub async fn db_cleanup(db: &mut PluginDb) -> anyhow::Result<()> {
    let used_keys: HashSet<_> = db
        .ides
        .values()
        .flat_map(|ides| {
            ides.iter()
                .map(|(name, version)| PluginVersion::new(name, version))
        })
        .collect();

    db.all_plugins = take(&mut db.all_plugins)
        .into_iter()
        .filter(|(k, _)| used_keys.contains(k))
        .collect();

    Ok(())
}
