use anyhow::{Context, Result};
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub stream_name: String,
    pub consumer_name: String,
    pub response_stream_name: String,
    pub tb_address: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            host: env::var("RABBITMQ_ADVERTISED_HOST")
                .context("RABBITMQ_ADVERTISED_HOST must be set")?,
            port: env::var("RABBITMQ_STREAM_PORT")
                .context("RABBITMQ_STREAM_PORT must be set")?
                .parse()
                .context("RABBITMQ_STREAM_PORT must be a valid number")?,
            username: env::var("RABBITMQ_DEFAULT_USER")
                .context("RABBITMQ_DEFAULT_USER must be set")?,
            password: env::var("RABBITMQ_DEFAULT_PASS")
                .context("RABBITMQ_DEFAULT_PASS must be set")?,
            stream_name: env::var("RABBITMQ_STREAM_NAME")
                .context("RABBITMQ_STREAM_NAME must be set")?,
            consumer_name: env::var("RABBITMQ_CONSUMER_NAME")
                .unwrap_or_else(|_| "rust_consumer".to_string()),
            response_stream_name: env::var("RABBITMQ_RESPONSE_STREAM_NAME")
                .context("RABBITMQ_RESPONSE_STREAM_NAME must be set")?,
            tb_address: env::var("TB_ADDRESS").context("TB_ADDRESS must be set")?,
        })
    }
}
