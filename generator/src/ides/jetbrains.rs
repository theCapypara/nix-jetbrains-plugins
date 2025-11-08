use crate::ides::{IdeProduct, IdeVersion, allowed_build_version};
use log::warn;
use serde::Deserialize;
use std::collections::HashSet;

const JETBRAINS_VERSIONS: &str = "https://www.jetbrains.com/updates/updates.xml";

#[derive(Debug, PartialEq, Deserialize)]
pub struct Products {
    product: Vec<Product>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct Product {
    code: Vec<String>,
    channel: Option<Vec<Channel>>,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct Channel {
    #[serde(rename = "@id")]
    id: String,
    build: Vec<Build>,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct Build {
    #[serde(rename = "@number")]
    number: String,
    #[serde(rename = "@fullNumber")]
    full_number: Option<String>,
    #[serde(rename = "@version")]
    version: String,
}

pub async fn collect_ids() -> anyhow::Result<Vec<IdeVersion>> {
    let products: Products =
        serde_xml_rs::from_str(&reqwest::get(JETBRAINS_VERSIONS).await?.text().await?)?;

    let mut already_processed = HashSet::new();
    let mut versions: Vec<IdeVersion> = Vec::new();

    for product in products.product {
        for code in product.code {
            if let Some(ideobj) = IdeProduct::try_from_code(&code)
                && already_processed.insert(ideobj)
                && let Some(channels) = product.channel.as_ref()
            {
                for channel in channels {
                    if channel.id.ends_with("RELEASE-licensing-RELEASE") {
                        for build in &channel.build {
                            if allowed_build_version(&build.version) {
                                versions.push(IdeVersion {
                                    ide: ideobj,
                                    version: build.version.clone(),
                                    build_number: build
                                        .full_number
                                        .as_ref()
                                        .map_or_else(|| build.number.clone(), Clone::clone),
                                })
                            } else {
                                warn!("Ignoring {} {}: too old", ideobj.nix_key(), build.version);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(versions)
}
