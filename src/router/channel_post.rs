use crate::config::Config;
use anyhow::Result;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::{debug, info};

pub async fn channel_post(config: Arc<Config>, post: Message) -> Result<()> {
    if !config.update_chat_list.contains(&post.chat.id) {
        debug!("Received channel post from unknown chat: {:?}", post.chat);
        return Ok(());
    }

    if let Some(text) = post.text() {
        info!("Received channel post: {}", text);
    } else {
        debug!("Ignoring channel post without text: {:?}", post.id);
    }

    Ok(())
}
