use crate::moodle::Moodle;
use crate::router::{MyDialogue, State};
use crate::MyBot;
use anyhow::Result;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use teloxide::utils::html::{bold, escape};
use tracing::{info, instrument, warn};

use super::Command;

#[instrument(skip_all, fields(tg.chat_id = %message.chat.id, tg.message_id = %message.id, tg.message = %message.text().unwrap_or("<no text>")))]
pub async fn help(bot: MyBot, message: Message) -> Result<()> {
    info!("Received help command from {}", message.chat.id);
    bot.send_message(message.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}
#[instrument(skip_all, fields(tg.chat_id = %message.chat.id, tg.message_id = %message.id, tg.message = %message.text().unwrap_or("<no text>")))]
pub async fn start(bot: MyBot, dialogue: MyDialogue, message: Message) -> Result<()> {
    info!("Received start command from {}", message.chat.id);

    bot.send_message(
        message.chat.id,
        "Let's start! Give me your moodle session\n[TODO: write instructions]",
    )
    .await?;
    dialogue.update(State::ReceiveSession).await?;

    Ok(())
}
#[instrument(skip_all, fields(tg.chat_id = %message.chat.id, tg.message_id = %message.id, tg.message = %message.text().unwrap_or("<no text>")))]
pub async fn reset(bot: MyBot, dialogue: MyDialogue, message: Message) -> Result<()> {
    info!("Received reset command from {}", message.chat.id);
    bot.send_message(
        message.chat.id,
        "Resetting the bot, you are no longer registered",
    )
    .await?;
    dialogue.update(State::Start).await?;
    Ok(())
}

#[instrument(skip_all, fields(tg.chat_id = %message.chat.id, tg.message_id = %message.id, tg.message = %message.text().unwrap_or("<no text>")))]
pub async fn receive_cookie(
    bot: MyBot,
    moodle: Arc<Moodle>,
    dialogue: MyDialogue,
    message: Message,
) -> Result<()> {
    info!("Received cookie from {}", message.chat.id);
    match message.text().map(ToOwned::to_owned) {
        Some(session) => {
            let message = bot
                .send_message(message.chat.id, "Checking session...")
                .await?;

            // TODO: check with regex and warn/error if it doesn't look like a session cookie
            match moodle.make_user(session).await {
                Ok(Some(user)) => {
                    let user_str = format!("{}", user);

                    dialogue.update(State::Registered(user)).await?;

                    bot.edit_message_text(
                        message.chat.id,
                        message.id,
                        format!(
                            "Hello, {}!\nYou are registered now. When the attendance password will be published, I will put a mark for you",
                            bold(&escape(&user_str))
                        ),
                    )
                    .await?;
                }
                Ok(None) => {
                    bot.edit_message_text(
                        message.chat.id,
                        message.id,
                        "Invalid session cookie, try again",
                    )
                    .await?;
                }
                Err(e) => {
                    warn!("Failed to make user: {:?}", e);
                    bot.edit_message_text(
                        message.chat.id,
                        message.id,
                        "Failed to contact moodle. You can try again or contact the bot admin",
                    )
                    .await?;
                }
            }
        }
        None => {
            bot.send_message(message.chat.id, "I need a text message, not this!")
                .await?;
        }
    }

    Ok(())
}
#[instrument(skip_all, fields(tg.chat_id = %message.chat.id, tg.message_id = %message.id, tg.message = %message.text().unwrap_or("<no text>")))]
pub async fn invalid_state(bot: MyBot, message: Message) -> Result<()> {
    bot.send_message(
        message.chat.id,
        "Unable to handle the message. Type /help to see the usage.\n\nMaybe you want to /start?",
    )
    .await?;
    Ok(())
}
