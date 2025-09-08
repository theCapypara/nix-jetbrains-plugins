use log::warn;
use serde::Deserialize;
use std::collections::HashSet;

const JETBRAINS_VERSIONS: &str = "https://www.jetbrains.com/updates/updates.xml";

const PROCESSED_VERSION_PREFIXES: &[&str] = &["2027.", "2026.", "2025.", "2024.3."];

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum IdeProduct {
    IntelliJUltimate,
    IntelliJCommunity,
    PhpStorm,
    WebStorm,
    PyCharmProfessional,
    PyCharmCommunity,
    RubyMine,
    CLion,
    GoLand,
    DataGrip,
    DataSpell,
    Rider,
    AndroidStudio,
    RustRover,
    Aqua,
    Writerside,
    Mps,
}
impl IdeProduct {
    fn try_from_code(code: &str) -> Option<Self> {
        Some(match code {
            "IU" => IdeProduct::IntelliJUltimate,
            "IC" => IdeProduct::IntelliJCommunity,
            "PS" => IdeProduct::PhpStorm,
            "WS" => IdeProduct::WebStorm,
            "PY" => IdeProduct::PyCharmProfessional,
            "PC" => IdeProduct::PyCharmCommunity,
            "RM" => IdeProduct::RubyMine,
            "CL" => IdeProduct::CLion,
            "GO" => IdeProduct::GoLand,
            "DB" => IdeProduct::DataGrip,
            "DS" => IdeProduct::DataSpell,
            "RD" => IdeProduct::Rider,
            "AI" => IdeProduct::AndroidStudio,
            "RR" => IdeProduct::RustRover,
            "QA" => IdeProduct::Aqua,
            "WRS" => IdeProduct::Writerside,
            "MPS" => IdeProduct::Mps,
            _ => return None,
        })
    }

    #[allow(unused)] // maybe useful later
    pub fn product_code(&self) -> &str {
        match self {
            IdeProduct::IntelliJUltimate => "IU",
            IdeProduct::IntelliJCommunity => "IC",
            IdeProduct::PhpStorm => "PS",
            IdeProduct::WebStorm => "WS",
            IdeProduct::PyCharmProfessional => "PY",
            IdeProduct::PyCharmCommunity => "PC",
            IdeProduct::RubyMine => "RM",
            IdeProduct::CLion => "CL",
            IdeProduct::GoLand => "GO",
            IdeProduct::DataGrip => "DB",
            IdeProduct::DataSpell => "DS",
            IdeProduct::Rider => "RD",
            IdeProduct::AndroidStudio => "AI",
            IdeProduct::RustRover => "RR",
            IdeProduct::Aqua => "QA",
            IdeProduct::Writerside => "WRS",
            IdeProduct::Mps => "MPS",
        }
    }

    fn try_from_nix_key(code: &str) -> Option<Self> {
        Some(match code {
            "idea-ultimate" => IdeProduct::IntelliJUltimate,
            "idea-community" => IdeProduct::IntelliJCommunity,
            "phpstorm" => IdeProduct::PhpStorm,
            "webstorm" => IdeProduct::WebStorm,
            "pycharm-professional" => IdeProduct::PyCharmProfessional,
            "pycharm-community" => IdeProduct::PyCharmCommunity,
            "ruby-mine" => IdeProduct::RubyMine,
            "clion" => IdeProduct::CLion,
            "goland" => IdeProduct::GoLand,
            "datagrip" => IdeProduct::DataGrip,
            "dataspell" => IdeProduct::DataSpell,
            "rider" => IdeProduct::Rider,
            "android-studio" => IdeProduct::AndroidStudio,
            "rust-rover" => IdeProduct::RustRover,
            "aqua" => IdeProduct::Aqua,
            "writerside" => IdeProduct::Writerside,
            "mps" => IdeProduct::Mps,
            _ => return None,
        })
    }

    pub fn nix_key(&self) -> &str {
        match self {
            IdeProduct::IntelliJUltimate => "idea-ultimate",
            IdeProduct::IntelliJCommunity => "idea-community",
            IdeProduct::PhpStorm => "phpstorm",
            IdeProduct::WebStorm => "webstorm",
            IdeProduct::PyCharmProfessional => "pycharm-professional",
            IdeProduct::PyCharmCommunity => "pycharm-community",
            IdeProduct::RubyMine => "ruby-mine",
            IdeProduct::CLion => "clion",
            IdeProduct::GoLand => "goland",
            IdeProduct::DataGrip => "datagrip",
            IdeProduct::DataSpell => "dataspell",
            IdeProduct::Rider => "rider",
            IdeProduct::AndroidStudio => "android-studio",
            IdeProduct::RustRover => "rust-rover",
            IdeProduct::Aqua => "aqua",
            IdeProduct::Writerside => "writerside",
            IdeProduct::Mps => "mps",
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct IdeVersion {
    pub ide: IdeProduct,
    pub version: String,
    pub build_number: String,
}

impl IdeVersion {
    /// Create from a JSON filename.
    /// WARNING: Does not populate build number!
    pub fn from_json_filename(filename: &str) -> Option<Self> {
        let filename = filename.strip_suffix(".json")?;
        let (product, version) = filename.rsplit_once('-')?;
        Some(Self {
            ide: IdeProduct::try_from_nix_key(product)?,
            version: version.to_string(),
            build_number: "".to_string(),
        })
    }

    pub fn to_json_filename(&self) -> String {
        format!("{}-{}.json", self.ide.nix_key(), self.version)
    }
}

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

fn allowed_build_version(version: &str) -> bool {
    for allowed in PROCESSED_VERSION_PREFIXES {
        if version.starts_with(allowed) {
            return true;
        }
    }
    false
}
