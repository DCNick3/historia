use crate::attendance::Attendance;
use crate::config::Config;
use crate::moodle::Moodle;
use crate::router::{MyStorage, State};
use crate::MyBot;
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::utils::html::{bold, code_inline};
use tracing::{debug, error, info};

static PASSWORD_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^Attendance password for (?P<day>\d+)\.(?P<month>\d+): (?P<password>.*)$").unwrap()
});

fn parse_attendance(text: &str) -> Option<Attendance> {
    PASSWORD_REGEX.captures(text).map(|cap| {
        let day = cap.name("day").unwrap().as_str().parse::<u8>().unwrap();
        let month = cap.name("month").unwrap().as_str().parse::<u8>().unwrap();
        let password = cap.name("password").unwrap().as_str().to_string();

        Attendance {
            day,
            month,
            password,
        }
    })
}

fn format_failure_message(attendance: &Attendance, reason: &str) -> String {
    format!(
        "I could not put an attendance mark for {} because {reason}.\nYou can manually go to [TODO] and use password {}",
        bold(&format!("{:02}.{:02}", attendance.day, attendance.month)), code_inline(&attendance.password)
    )
}

async fn handle_user(
    bot: &MyBot,
    moodle: &Moodle,
    chat_id: ChatId,
    state: State,
    attendance: &Attendance,
) -> Result<()> {
    match state {
        State::Start => {
            // missed attendance because not registered
            // TODO: better format
            bot.send_message(
                chat_id,
                format_failure_message(attendance, "you are not registered"),
            )
            .await?;
        }
        State::ReceiveSession => {
            // don't interrupt the user
        }
        State::Registered(user) => match moodle.mark_attendance(&user, attendance).await {
            Ok(_) => {
                bot.send_message(
                    chat_id,
                    format!(
                        "Marked your attendance for {}",
                        bold(&format!("{:02}.{:02}", attendance.day, attendance.month))
                    ),
                )
                .await?;
            }
            Err(e) => {
                error!(
                    "Failed to mark attendance for {}/{}: {:?}",
                    chat_id, user, e
                );
                bot.send_message(
                    chat_id,
                    format_failure_message(attendance, "of some nasty error"),
                )
                .await?;
            }
        },
    }

    Ok(())
}

pub async fn channel_post(
    bot: MyBot,
    config: Arc<Config>,
    moodle: Arc<Moodle>,
    post: Message,
    storage: Arc<MyStorage>,
) -> Result<()> {
    if !config.update_chat_list.contains(&post.chat.id) {
        debug!("Received channel post from unknown chat: {:?}", post.chat);
        return Ok(());
    }

    if let Some(text) = post.text() {
        if let Some(attendance) = parse_attendance(text) {
            info!("Received password: {}", attendance);

            let dialogues = storage.get_all_dialogues::<State>().await?;
            info!("Found {} dialogues", dialogues.len());

            for (chat_id, state) in dialogues {
                if !chat_id.is_user() {
                    continue;
                }

                if let Err(e) = handle_user(&bot, &moodle, chat_id, state, &attendance).await {
                    error!("Failed to handle user {}: {:?}", chat_id, e);
                }
            }
        } else {
            debug!(
                "Received channel post from {:?} with unknown text: {:?}",
                post.chat, text
            );
        }
    } else {
        debug!("Ignoring channel post without text: {:?}", post.id);
    }

    Ok(())
}
