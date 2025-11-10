mod android_studio;
mod jetbrains;

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

pub async fn collect_ids() -> anyhow::Result<Vec<IdeVersion>> {
    let (jetbrains, android_studio) =
        tokio::try_join!(jetbrains::collect_ids(), android_studio::collect_ids())?;

    Ok([jetbrains, android_studio].concat())
}

fn allowed_build_version(version: &str) -> bool {
    for allowed in PROCESSED_VERSION_PREFIXES {
        if version.starts_with(allowed) {
            return true;
        }
    }
    false
}
