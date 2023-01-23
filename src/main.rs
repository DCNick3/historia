mod config;
mod router;

use dptree::deps;
use std::sync::Arc;
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::prelude::*;
use tracing::info;

use router::{schema, State};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    info!("Starting throw dice bot...");

    let bot = Bot::from_env();
    let listener = teloxide::update_listeners::polling_default(bot.clone()).await;

    let config = config::Config {
        update_chat_list: vec![ChatId(-1001842503691), ChatId(-1001727873081)],
    };

    Dispatcher::builder(bot, schema())
        .dependencies(deps![Arc::new(config), InMemStorage::<State>::new()])
        .enable_ctrlc_handler()
        .build()
        .dispatch_with_listener(
            listener,
            LoggingErrorHandler::with_custom_text("An error from the update listener"),
        )
        .await;
}
