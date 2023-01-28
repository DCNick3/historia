use crate::moodle::{Moodle, SessionProbeResult};
use crate::router::{MyDialogue, MyStorage, State};
use crate::{config, MyBot};
use anyhow::{Context, Result};
use std::borrow::Cow;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use teloxide::utils::html::{bold, code_inline, escape, link};
use tracing::{error, info, instrument, warn};

use super::Command;

#[instrument(skip_all, fields(tg.chat_id = %message.chat.id, tg.message_id = %message.id, tg.message = %message.text().unwrap_or("<no text>")))]
pub async fn help(bot: MyBot, message: Message) -> Result<()> {
    info!("Received help command from {}", message.chat.id);
    bot.send_message(message.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}
#[instrument(skip_all, fields(tg.chat_id = %message.chat.id, tg.message_id = %message.id, tg.message = %message.text().unwrap_or("<no text>")))]
pub async fn start(
    bot: MyBot,
    moodle_config: Arc<config::Moodle>,
    dialogue: MyDialogue,
    message: Message,
) -> Result<()> {
    info!("Received start command from {}", message.chat.id);

    bot.send_message(
        message.chat.id,
        format!("Let's start! Give me your moodle session\n\n\
        To get it, you should navigate to the moodle page & log in:\n\n{}\n\nThen, open the developer tools (F12) and print cookies by entering {} in the console.\n\n\
        The output should look like this:\n\n{}\n\nOr like this:\n\n{}\n\n\
        Copy the value of the {} cookie (everything after {}, for example {}) and send it to me.",
                link(moodle_config.base_url.as_str(), moodle_config.base_url.as_str()),
                code_inline("document.cookie"),
                code_inline("\"MoodleSession=1234567890\""),
                code_inline("\"MoodleSession=1234567890; SomeOtherCookie=lol\""),
                bold("MoodleSession"),
                code_inline("MoodleSession="),
                bold("1234567890"),
        ),
    )
        .await?;
    dialogue.update(State::ReceiveSession).await?;

    Ok(())
}
#[instrument(skip_all, fields(tg.chat_id = %message.chat.id, tg.message_id = %message.id, tg.message = %message.text().unwrap_or("<no text>")))]
pub async fn status(
    bot: MyBot,
    moodle: Arc<Moodle>,
    dialogue: MyDialogue,
    message: Message,
) -> Result<()> {
    info!("Received status command from {}", message.chat.id);

    let status = dialogue.get().await.context("Getting status")?;
    let status: Cow<_> = match status.unwrap() {
        State::Start => "You are not registered yet. Use /start to register\n\n🚫 You will NOT be marked".into(),
        State::ReceiveSession => {
            "You are not registered yet. Send me your moodle session cookie to register\n\n🚫 You will NOT be marked".into()
        }
        State::Registered(user) => {
            let result = moodle.check_user(&user).await;

            match result {
                Ok(SessionProbeResult::Valid { .. }) => {
                    format!(
                        "You are registered as {}. You can use /reset to unregister\n\n✅ You WILL be marked",
                        user
                    )
                        .into()
                }
                Ok(SessionProbeResult::Invalid) => {
                    warn!("Session invalidated");
                    dialogue.update(State::Start).await?;
                    "You were registered, but your moodle session has expired. Use /start to re-register\n\n🚫 You will NOT be marked"
                        .into()
                }
                Err(e) => {
                    error!("Error while checking user: {}", e);
                    "An error occurred while checking your moodle session. Try again later\n\n⁉️ You will be marked MAYBE??"
                        .into()
                }
            }
        },
    };

    bot.send_message(message.chat.id, status).await?;

    Ok(())
}
#[instrument(skip_all, fields(tg.chat_id = %message.chat.id, tg.message_id = %message.id, tg.message = %message.text().unwrap_or("<no text>")))]
pub async fn super_status(
    bot: MyBot,
    moodle: Arc<Moodle>,
    storage: Arc<MyStorage>,
    message: Message,
) -> Result<()> {
    info!("Received super_status command from {}", message.chat.id);

    let mut status = String::new();

    let statuses = storage.get_all_dialogues::<State>().await?;
    for (&chat, state) in statuses.iter() {
        if !chat.is_user() {
            continue;
        }

        let state: Cow<_> = match state {
            State::Start => "[unregistered]".into(),
            State::ReceiveSession => "[unregistered]".into(),
            State::Registered(user) => {
                let result = moodle.check_user(user).await;

                let result = match result {
                    Ok(SessionProbeResult::Valid { .. }) => "VALID",
                    Ok(SessionProbeResult::Invalid) => "INVAL",
                    Err(e) => {
                        error!("Error while checking user: {}", e);
                        "ERROR"
                    }
                };

                format!("{} [{}]", user, result).into()
            }
        };

        status.push_str(&format!(
            "{}: {}\n",
            code_inline(&chat.to_string()),
            escape(&state)
        ));
    }

    bot.send_message(message.chat.id, status).await?;

    Ok(())
}
#[instrument(skip_all, fields(tg.chat_id = %message.chat.id, tg.message_id = %message.id, tg.message = %message.text().unwrap_or("<no text>")))]
pub async fn tell(
    bot: MyBot,
    storage: Arc<MyStorage>,
    message: Message,
    global_message: String,
) -> Result<()> {
    info!(
        "Received tell command from {}: {}",
        message.chat.id, global_message
    );

    let statuses = storage.get_all_dialogues::<State>().await?;
    for &chat in statuses.keys() {
        if chat.is_user() {
            bot.send_message(chat, &global_message).await?;
        }
    }

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
