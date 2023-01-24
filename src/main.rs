mod attendance;
mod config;
mod init_tracing;
mod moodle;
mod moodle_extender;
mod reqwest_span_backend;
mod router;
mod storage;
mod teloxide_tracing;

use crate::moodle::Moodle;
use crate::moodle_extender::MoodleExtender;
use anyhow::{Context, Result};
use dptree::deps;
use router::{schema, MyStorage};
use std::sync::Arc;
use std::time::Duration;
use teloxide::adaptors::{DefaultParseMode, Throttle};
use teloxide::dispatching::dialogue::serializer::Json;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::update_listeners::Polling;
use teloxide_tracing::Trace;
use tracing::info;

type MyBot = Trace<Throttle<DefaultParseMode<Bot>>>;

fn make_bot() -> Result<Bot> {
    let token_file = std::env::var("TELOXIDE_TOKEN_FILE")
        .context("TELOXIDE_TOKEN_FILE is not set (should contain path to file with bot token)")?;
    let token = std::fs::read_to_string(token_file).context("Reading token file")?;

    Ok(Bot::new(token.trim()))
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing::init_tracing().context("Setting up the opentelemetry exporter")?;
    info!("Starting historia bot...");

    let config = config::Config::read()?;

    let bot: MyBot = Trace::new(
        make_bot()?
            .parse_mode(ParseMode::Html)
            .throttle(Default::default()),
        teloxide_tracing::Settings::all(),
    );

    let listener = Polling::builder(bot.clone())
        .timeout(Duration::from_secs(10))
        .delete_webhook()
        .await
        .build();

    let storage = MyStorage::open(&config.database, Json)
        .await
        .context("Opening storage")?;

    let moodle_extender = MoodleExtender::new(&config.moodle_extender).await?;

    let moodle = Arc::new(
        Moodle::new(&config.moodle, moodle_extender)
            .await
            .context("Opening moodle accessor")?,
    );

    Dispatcher::builder(bot, schema())
        .dependencies(deps![
            Arc::new(config.bot),
            Arc::new(config.moodle),
            storage,
            moodle
        ])
        .enable_ctrlc_handler()
        .build()
        .dispatch_with_listener(
            listener,
            LoggingErrorHandler::with_custom_text("An error from the update listener"),
        )
        .await;

    Ok(())
}
