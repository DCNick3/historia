mod channel_post;
mod commands;

use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::dispatching::{dialogue, UpdateHandler};
use teloxide::macros::BotCommands;
use teloxide::prelude::*;

use crate::router::commands::{invalid_state, receive_cookie};
use channel_post::channel_post;
use commands::{cancel, help, start};

#[derive(Clone, Default)]
pub enum State {
    #[default]
    Start,
    ReceiveCookie,
    ReceiveProductChoice {
        full_name: String,
    },
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
    #[command(description = "cancel the registration procedure.")]
    Cancel,
}

fn is_text(message: &Message) -> bool {
    message.text().is_some()
}

pub fn schema() -> UpdateHandler<anyhow::Error> {
    use dptree::case;

    let command_handler = teloxide::filter_command::<Command, _>()
        .branch(
            case![State::Start]
                .branch(case![Command::Help].endpoint(help))
                .branch(case![Command::Start].endpoint(start)),
        )
        .branch(case![Command::Cancel].endpoint(cancel));

    let message_handler = Update::filter_message()
        .branch(command_handler)
        .branch(
            case![State::ReceiveCookie]
                .filter(is_text)
                .endpoint(receive_cookie),
        )
        .branch(dptree::endpoint(invalid_state));

    let channel_post_handler = Update::filter_channel_post().endpoint(channel_post);

    dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(message_handler)
        .branch(channel_post_handler)
}
