use crate::moodle::{Moodle, MoodleError};
use crate::router::{MyDialogue, State};
use crate::MyBot;
use anyhow::Result;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use teloxide::utils::html::{bold, escape};
use tracing::{info, warn};

use super::Command;

pub async fn help(bot: MyBot, message: Message) -> Result<()> {
    info!("Received help command from {:?}", message.chat);
    bot.send_message(message.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}
pub async fn start(bot: MyBot, dialogue: MyDialogue, message: Message) -> Result<()> {
    info!("Received start command from {:?}", message.chat);

    bot.send_message(
        message.chat.id,
        "Let's start! Give me your moodle session\n[TODO: write instructions]",
    )
    .await?;
    dialogue.update(State::ReceiveSession).await?;

    Ok(())
}
pub async fn cancel(bot: MyBot, dialogue: MyDialogue, message: Message) -> Result<()> {
    info!("Received cancel command from {:?}", message.chat);
    bot.send_message(message.chat.id, "Cancelling the dialogue.")
        .await?;
    dialogue.exit().await?;
    Ok(())
}

pub async fn receive_cookie(
    bot: MyBot,
    moodle: Arc<Moodle>,
    dialogue: MyDialogue,
    message: Message,
) -> Result<()> {
    info!("Received cookie from {:?}", message.chat);
    match message.text().map(ToOwned::to_owned) {
        Some(session) => {
            let message = bot
                .send_message(message.chat.id, "Checking session...")
                .await?;

            // TODO: check with regex and warn/error if it doesn't look like a session cookie
            match moodle.make_user(session).await {
                Ok(user) => {
                    let user_str = format!("{}", user);

                    dialogue.update(State::Registered(user)).await?;

                    bot.edit_message_text(
                        message.chat.id,
                        message.id,
                        format!(
                            "Hello, {}!\nYou are registered now",
                            bold(&escape(&user_str))
                        ),
                    )
                    .await?;
                }
                Err(MoodleError::SessionInvalid) => {
                    bot.edit_message_text(message.chat.id, message.id, "Invalid session cookie")
                        .await?;
                }
                Err(e) => {
                    warn!("Failed to make user: {:?}", e);
                    bot.edit_message_text(
                        message.chat.id,
                        message.id,
                        "Failed to contact moodle. Contact the bot owner",
                    )
                    .await?;
                }
            }
        }
        None => {
            bot.send_message(message.chat.id, "I need a cookie!")
                .await?;
        }
    }

    Ok(())
}
pub async fn invalid_state(bot: MyBot, message: Message) -> Result<()> {
    warn!("Invalid state!11");
    bot.send_message(
        message.chat.id,
        "Unable to handle the message. Type /help to see the usage.",
    )
    .await?;
    Ok(())
}
