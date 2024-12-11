mod ides;
mod logging;
mod plugins;

use clap::{Parser, Subcommand};
use log::info;
use std::path::PathBuf;
use tokio::try_join;

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    output_path: PathBuf,
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate the IDE JSON files and create/update all_plugins.json
    Generate,
    /// Remove all plugins from all_plugins.json that are no longer used in any IDE json file.
    Cleanup,
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

    match cli.command {
        Command::Generate => generate(cli).await,
        Command::Cleanup => cleanup(cli).await,
    }
}

async fn generate(cli: Cli) -> anyhow::Result<()> {
    info!("running generate.");
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
    let mut db = plugins::db_load(&cli.output_path).await?;
    info!("Beginning plugin download...");
    plugins::db_update(&mut db, &ides, &plugins).await?;
    info!("Saving DB...");
    plugins::db_save(&cli.output_path, db).await?;

    Ok(())
}

async fn cleanup(cli: Cli) -> anyhow::Result<()> {
    info!("Loading database and IDE mappings.");
    let mut db = plugins::db_load_full(&cli.output_path).await?;

    info!("Running cleanup...");
    plugins::db_cleanup(&mut db).await?;

    info!("Saving DB...");
    plugins::db_save(&cli.output_path, db).await?;

    Ok(())
}
