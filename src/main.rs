mod attendance;
mod config;
mod moodle;
mod moodle_extender;
mod router;
mod storage;
mod time_trace;

use anyhow::Context;
use dptree::deps;
use std::sync::Arc;
use std::time::Duration;
use teloxide::adaptors::{DefaultParseMode, Throttle, Trace};
use teloxide::dispatching::dialogue::serializer::Json;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::update_listeners::Polling;
use tracing::info;

use crate::moodle::Moodle;
use crate::moodle_extender::MoodleExtender;
use router::{schema, MyStorage};

type MyBot = Trace<Throttle<DefaultParseMode<Bot>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    info!("Starting historia bot...");

    let config = config::Config::read()?;

    let bot: MyBot = Bot::from_env()
        .parse_mode(ParseMode::Html)
        .throttle(Default::default())
        .trace(teloxide::adaptors::trace::Settings::all());

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
        .dependencies(deps![Arc::new(config.bot), storage, moodle])
        .enable_ctrlc_handler()
        .build()
        .dispatch_with_listener(
            listener,
            LoggingErrorHandler::with_custom_text("An error from the update listener"),
        )
        .await;

    Ok(())
}
