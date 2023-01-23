use serde::{Deserialize, Serialize};
use teloxide::types::ChatId;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub update_chat_list: Vec<ChatId>,
}
