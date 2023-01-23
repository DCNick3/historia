use anyhow::Result;
use teloxide::prelude::*;
use tracing::{info, warn};

pub async fn help(message: Message) -> Result<()> {
    info!("Received help command from {:?}", message.chat);
    Ok(())
}
pub async fn start(message: Message) -> Result<()> {
    info!("Received start command from {:?}", message.chat);
    Ok(())
}
pub async fn cancel(message: Message) -> Result<()> {
    info!("Received cancel command from {:?}", message.chat);
    Ok(())
}

pub async fn receive_cookie(message: Message) -> Result<()> {
    info!("Received cookie from {:?}", message.chat);
    Ok(())
}
pub async fn invalid_state() -> Result<()> {
    warn!("Invalid state!11");
    Ok(())
}
