use anyhow::Context;
use camino::Utf8PathBuf;
use serde::{de, Deserialize, Deserializer};
use std::str::FromStr;
use std::time::Duration;
use teloxide::types::ChatId;
use url::Url;

fn deserialize_path<'de, D>(de: D) -> Result<Utf8PathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = de::Deserialize::deserialize(de)?;
    Ok(Utf8PathBuf::from(s))
}

fn deserialize_url<'de, D>(de: D) -> Result<Url, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &'de str = de::Deserialize::deserialize(de)?;
    Url::parse(s).map_err(de::Error::custom)
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub database: Database,
    pub moodle: Moodle,
    pub moodle_extender: MoodleExtender,
    pub updater: Updater,
    pub bot: Bot,
}

impl Config {
    pub fn read() -> anyhow::Result<Config> {
        let config_path = std::env::var("CONFIG")
            .map(Utf8PathBuf::from)
            .unwrap_or_else(|_| Utf8PathBuf::from_str("config.yaml").unwrap());

        let config = std::fs::read_to_string(config_path).context("Reading config file")?;
        serde_yaml::from_str(&config).context("Parsing config file")
    }
}

#[derive(Debug, Deserialize)]
pub struct Database {
    #[serde(deserialize_with = "deserialize_path")]
    pub path: Utf8PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct Moodle {
    #[serde(deserialize_with = "deserialize_url")]
    pub base_url: Url,
    pub rpm: u32,
    pub max_burst: u32,
    pub user_agent: String,
    pub activity_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct MoodleExtender {
    #[serde(deserialize_with = "deserialize_url")]
    pub base_url: Url,
}

#[derive(Debug, Deserialize)]
pub struct Updater {
    #[serde(with = "humantime_serde")]
    pub interval: Duration,
}

#[derive(Debug, Deserialize)]
pub struct Bot {
    pub update_channels: Vec<BotChannel>,
}

#[derive(Debug, Deserialize)]
pub struct BotChannel {
    pub id: ChatId,
    pub activity_id: u32,
}
