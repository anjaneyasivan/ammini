use anyhow::{Context, Result};
use std::env;

/// Telegram API credentials loaded from .env file or environment variables.
#[derive(Debug, Clone)]
pub struct TelegramConfig {
    pub api_id: i32,
    pub api_hash: String,
}

impl TelegramConfig {
    /// Load configuration from, in order of precedence: the live environment,
    /// a `.env` file, then the values baked in at compile time by `build.rs`.
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let api_id = env::var("TELEGRAM_API_ID")
            .ok()
            .or_else(|| option_env!("TELEGRAM_API_ID").map(str::to_owned))
            .context("TELEGRAM_API_ID not set in environment, .env file, or build")?
            .parse::<i32>()
            .context("TELEGRAM_API_ID must be a valid integer")?;

        let api_hash = env::var("TELEGRAM_API_HASH")
            .ok()
            .or_else(|| option_env!("TELEGRAM_API_HASH").map(str::to_owned))
            .context("TELEGRAM_API_HASH not set in environment, .env file, or build")?;

        if api_hash.is_empty() {
            anyhow::bail!("TELEGRAM_API_HASH must not be empty");
        }

        Ok(TelegramConfig { api_id, api_hash })
    }
}
