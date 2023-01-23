mod attendance;
mod config;
mod moodle;
mod router;
mod storage;

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
use router::{schema, MyStorage};

type MyBot = Trace<Throttle<DefaultParseMode<Bot>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    info!("Starting throw dice bot...");

    let bot: MyBot = Bot::from_env()
        .parse_mode(ParseMode::Html)
        .throttle(Default::default())
        .trace(teloxide::adaptors::trace::Settings::all());

    let listener = Polling::builder(bot.clone())
        .timeout(Duration::from_secs(10))
        .delete_webhook()
        .await
        .build();

    let config = Arc::new(config::Config {
        update_chat_list: vec![ChatId(-1001842503691), ChatId(-1001727873081)],
    });

    let storage = MyStorage::open("storage.db", Json)
        .await
        .context("Opening storage")?;

    let moodle = Arc::new(Moodle::new().await.context("Opening moodle accessor")?);

    Dispatcher::builder(bot, schema())
        .dependencies(deps![config, storage, moodle])
        .enable_ctrlc_handler()
        .build()
        .dispatch_with_listener(
            listener,
            LoggingErrorHandler::with_custom_text("An error from the update listener"),
        )
        .await;

    Ok(())
}
