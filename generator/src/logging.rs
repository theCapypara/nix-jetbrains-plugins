use log::LevelFilter;
use log4rs::append::console::{ConsoleAppender, Target};
use log4rs::config::{Appender, Root};
use log4rs::{init_config, Config, Handle};

pub fn setup_logging() -> anyhow::Result<Handle> {
    let threshold = if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };

    let config = Config::builder()
        .appender(Appender::builder().build(
            "stderr",
            Box::new(ConsoleAppender::builder().target(Target::Stderr).build()),
        ))
        .build(Root::builder().appender("stderr").build(threshold))?;

    Ok(init_config(config)?)
}
