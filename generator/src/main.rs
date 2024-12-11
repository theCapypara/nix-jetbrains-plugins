mod ides;
mod logging;
mod plugins;

use clap::Parser;
use log::info;
use std::path::PathBuf;
use tokio::try_join;

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    output_path: PathBuf,
}
const PLUGIN_INDICES: &[&str] = &[
    "https://downloads.marketplace.jetbrains.com/files/pluginsXMLIds.json",
    "https://downloads.marketplace.jetbrains.com/files/jbPluginsXMLIds.json",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    _ = logging::setup_logging();
    info!("Starting...");

    let (ides, mut plugins, jb_plugins) = try_join!(
        ides::collect_ids(),
        plugins::index(PLUGIN_INDICES[0]),
        plugins::index(PLUGIN_INDICES[1])
    )?;

    info!(
        "Indexing {} IDE versions, {} plugins and {} Jetbrains plugins.",
        ides.len(),
        plugins.len(),
        jb_plugins.len()
    );
    plugins.extend_from_slice(&jb_plugins);

    info!("Loading old database.");
    let cache_db = plugins::db_cache_load(&cli.output_path).await?;
    info!("Beginning plugin download...");
    let db = plugins::build_db(&cache_db, &ides, &plugins).await?;
    info!("Saving DB...");
    plugins::db_save(&cli.output_path, db).await?;

    Ok(())
}
