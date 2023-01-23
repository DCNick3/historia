use crate::attendance::Attendance;
use crate::config::BotChannel;
use crate::moodle::{Moodle, SessionProbeResult};
use crate::router::{MyStorage, State};
use crate::{config, MyBot};
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Url;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::utils::html::{bold, code_inline, link};
use tracing::{debug, error, info, instrument};

static PASSWORD_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^Attendance password for (?P<day>\d+)\.(?P<month>\d+): (?P<password>.*)$").unwrap()
});

fn parse_attendance(text: &str) -> Option<Attendance> {
    PASSWORD_REGEX.captures(text).and_then(|cap| {
        let day = cap.name("day").unwrap().as_str().parse::<u8>().ok()?;
        let month = cap.name("month").unwrap().as_str().parse::<u8>().ok()?;
        let password = cap.name("password").unwrap().as_str().to_string();

        Some(Attendance {
            day,
            month,
            password,
        })
    })
}

enum Solutions {
    ManuallyMarkAt,
    ReRegister,
    Register,
}

fn format_failure_message(
    attendance: &Attendance,
    reason: &str,
    solutions: Solutions,
    manual_url: &Url,
) -> String {
    format!(
        "I could not put an attendance mark for {} because {reason}.\n\n{}",
        bold(&format!("{:02}.{:02}", attendance.day, attendance.month)),
        match solutions {
            Solutions::ManuallyMarkAt => {
                format!(
                    "Please do it manually, the password is {}\n\n{}",
                    code_inline(&attendance.password),
                    link(manual_url.as_str(), manual_url.as_str()),
                )
            }
            Solutions::ReRegister => {
                format!("You should re-register with /start command to not miss further attendance marks.\n\nFor now you can do it manually, the password is {}\n\n{}", code_inline(&attendance.password), link(manual_url.as_str(), manual_url.as_str()))
            }
            Solutions::Register => {
                format!("You should register with /start command to not miss further attendance marks.\n\nFor now you can do it manually, the password is {}\n\n{}", code_inline(&attendance.password), link(manual_url.as_str(), manual_url.as_str()))
            }
        }
    )
}

#[instrument(skip_all, fields(%chat_id))]
async fn handle_user(
    bot: &MyBot,
    moodle: &Moodle,
    activity_id: u32,
    chat_id: ChatId,
    state: State,
    attendance: &Attendance,
) -> Result<()> {
    match state {
        State::Start => {
            // missed attendance because not registered, suggest to register
            bot.send_message(
                chat_id,
                format_failure_message(
                    attendance,
                    "you are not registered",
                    Solutions::Register,
                    &moodle.make_attendance_url(activity_id)?,
                ),
            )
            .await?;
        }
        State::ReceiveSession => {
            // don't interrupt the user
        }
        State::Registered(user) => {
            let SessionProbeResult::Valid { csrf_session, email } = moodle.check_user(&user).await?
            else {
                bot.send_message(
                    chat_id,
                    format_failure_message(attendance, "your session has become invalid", Solutions::ReRegister,
                                           &moodle.make_attendance_url(activity_id)?),
                ).await?;
                return Ok(())
            };

            info!("Marking attendance for {}...", email);

            let sessions = match moodle
                .get_attendance_sessions(activity_id, &user)
                .await
                .map(|s| {
                    s.into_iter()
                        .filter(|s| s.matches(attendance))
                        .collect::<Vec<_>>()
                }) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to get attendance sessions: {}", e);
                    bot.send_message(
                        chat_id,
                        format_failure_message(
                            attendance,
                            "I failed to get attendance sessions",
                            Solutions::ManuallyMarkAt,
                            &moodle.make_attendance_url(activity_id)?,
                        ),
                    )
                    .await?;
                    return Ok(());
                }
            };

            if sessions.is_empty() {
                error!("No matching attendance sessions found");
                bot.send_message(
                    chat_id,
                    format_failure_message(
                        attendance,
                        "I failed to find matching attendance sessions (or you are already marked)",
                        Solutions::ManuallyMarkAt,
                        &moodle.make_attendance_url(activity_id)?,
                    ),
                )
                .await?;
                return Ok(());
            }

            info!("Matching sessions: {:?}", sessions);

            for session in sessions {
                match moodle
                    .mark_attendance_session(&user, &csrf_session, session.id, &attendance.password)
                    .await
                {
                    Ok(_) => {
                        info!("Marked attendance for {}", email);
                        bot.send_message(
                            chat_id,
                            format!(
                                "Attendance on {} marked successfully!",
                                bold(&format!("{:02}.{:02}", attendance.day, attendance.month))
                            ),
                        )
                        .await?;
                    }
                    Err(e) => {
                        error!("Failed to mark attendance: {}", e);
                        bot.send_message(
                            chat_id,
                            format_failure_message(
                                attendance,
                                "of some nasty error",
                                Solutions::ManuallyMarkAt,
                                &moodle.make_session_url(session.id)?,
                            ),
                        )
                        .await?;
                    }
                }
            }
        }
    }

    Ok(())
}

pub async fn channel_post(
    bot: MyBot,
    config: Arc<config::Bot>,
    moodle: Arc<Moodle>,
    post: Message,
    storage: Arc<MyStorage>,
) -> Result<()> {
    let Some(&BotChannel {
        activity_id,
        ..
    }) = config.update_channels.iter().find(|v| v.id == post.chat.id)
    else {
        debug!("Received channel post from unknown chat: {:?}", post.chat);
        return Ok(());
    };

    let Some(text) = post.text() else {
        debug!("Ignoring channel post without text: {:?}", post.id);
        return Ok(());
    };
    let Some(attendance) = parse_attendance(text) else {
        debug!(
            "Received channel post from {:?} with unknown text: {:?}",
            post.chat.id, text
        );
        return Ok(());
    };

    info!("Received password: {}", attendance);

    let dialogues = storage.get_all_dialogues::<State>().await?;
    info!("Found {} dialogues", dialogues.len());

    for (chat_id, state) in dialogues {
        if !chat_id.is_user() {
            continue;
        }

        if let Err(e) = handle_user(&bot, &moodle, activity_id, chat_id, state, &attendance).await {
            error!("Failed to handle user {}: {:?}", chat_id, e);
            // try to notify the user one last time
            let _ = bot.send_message(chat_id, "Some really nasty error happened when trying to mark attendance for you. You should go & check your attendance").await;
        }
    }

    Ok(())
}
