use crate::ides::{IdeProduct, IdeVersion, allowed_build_version};
use anyhow::anyhow;
use log::warn;
use serde::Deserialize;

const ANDROID_STUDIO_VERSIONS: &str = "https://jb.gg/android-studio-releases-list.json";

#[derive(Debug, PartialEq, Deserialize)]
pub struct Body {
    content: Content,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct Content {
    item: Vec<Item>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct Item {
    version: String,
    build: String,
    #[serde(rename = "platformBuild")]
    platform_build: String,
    channel: String,
}

pub async fn collect_ids() -> anyhow::Result<Vec<IdeVersion>> {
    let body: Body =
        serde_json::from_str(&reqwest::get(ANDROID_STUDIO_VERSIONS).await?.text().await?)?;

    let mut versions: Vec<IdeVersion> = Vec::new();

    for item in body.content.item {
        if !item.build.starts_with("AI-") {
            return Err(anyhow!(
                "Unexpected product code: {} don't start with AI",
                item.build
            ));
        }
        // Allow all `item.channel` because they are available in nixpkgs.

        if allowed_build_version(&item.version) {
            versions.push(IdeVersion {
                ide: IdeProduct::AndroidStudio,
                version: item.version,
                build_number: item.platform_build,
            })
        } else {
            warn!(
                "Ignoring {} {}: too old",
                IdeProduct::AndroidStudio.nix_key(),
                item.version
            );
        }
    }

    Ok(versions)
}
