mod channel_post;
mod commands;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use teloxide::dispatching::dialogue::serializer::Json;
use teloxide::dispatching::{dialogue, UpdateHandler};
use teloxide::macros::BotCommands;
use teloxide::prelude::*;
use teloxide::utils::command::ParseError;

use crate::config;
use crate::moodle::MoodleUser;
use crate::router::commands::{invalid_state, receive_cookie, status, super_status, tell};
use crate::storage::SqliteStorage;
use channel_post::channel_post;
use commands::{help, reset, start};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum State {
    #[default]
    Start,
    ReceiveSession,
    Registered(MoodleUser),
}

pub type MyStorage = SqliteStorage<Json>;
type MyDialogue = Dialogue<State, MyStorage>;

fn parse_tell(msg: String) -> Result<(String,), ParseError> {
    Ok((msg,))
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "start the registration procedure.")]
    Start,
    #[command(description = "check your token status.")]
    Status,
    #[command(description = "off")]
    SuperStatus,
    #[command(description = "off", parse_with = parse_tell)]
    Tell(String),
    #[command(description = "reset the bot, removing your registration.")]
    Reset,
}

pub fn schema(config: &config::Bot) -> UpdateHandler<anyhow::Error> {
    use dptree::case;

    let super_users = Arc::new(config.super_users.iter().cloned().collect::<HashSet<_>>());

    let is_superuser = move |m: Message| super_users.contains(&m.chat.id);

    let command_handler = teloxide::filter_command::<Command, _>()
        .branch(case![Command::Help].endpoint(help))
        .branch(case![Command::Start].endpoint(start))
        .branch(case![Command::Status].endpoint(status))
        .branch(case![Command::Reset].endpoint(reset))
        .branch(
            dptree::filter(is_superuser)
                .branch(case![Command::SuperStatus].endpoint(super_status))
                .branch(case![Command::Tell(message)].endpoint(tell)),
        );

    let message_handler = Update::filter_message()
        .filter(|m: Message| m.chat.id.is_user())
        .branch(command_handler)
        .branch(case![State::ReceiveSession].endpoint(receive_cookie))
        .branch(dptree::endpoint(invalid_state));

    let channel_post_handler = Update::filter_channel_post().endpoint(channel_post);

    dialogue::enter::<Update, MyStorage, State, _>()
        .branch(message_handler)
        .branch(channel_post_handler)
}
