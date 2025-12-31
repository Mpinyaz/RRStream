pub mod config;
pub mod services;
pub mod worker;
pub mod task {
    tonic::include_proto!("task");
}
use anyhow::anyhow;
use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    // Transient errors (should retry)
    NetworkTimeout,
    DatabaseUnavailable,
    TemporaryOverload,

    // Permanent errors (should not retry)
    InvalidTaskFormat,
    InvalidInput,
    AccountAlreadyExists,
    InsufficientBalance,
    InvalidOperation,
    ProtobufDecode,
    TigerBeetle,
    RabbitMQ,
}
impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl ErrorKind {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ErrorKind::NetworkTimeout
                | ErrorKind::DatabaseUnavailable
                | ErrorKind::TemporaryOverload
        )
    }

    pub fn retry_delay_ms(&self) -> u64 {
        match self {
            ErrorKind::NetworkTimeout => 100,
            ErrorKind::DatabaseUnavailable => 1000,
            ErrorKind::TemporaryOverload => 500,
            _ => 0,
        }
    }
}

pub trait ErrorKindExt {
    fn with_kind(self, kind: ErrorKind) -> anyhow::Error;
}

impl ErrorKindExt for anyhow::Error {
    fn with_kind(self, kind: ErrorKind) -> anyhow::Error {
        self.context(format!("ErrorKind::{kind:?}"))
    }
}

pub fn get_error_kind(error: &anyhow::Error) -> Option<ErrorKind> {
    for cause in error.chain() {
        let err_str = format!("{cause:?}");
        if err_str.contains("ErrorKind::NetworkTimeout") {
            return Some(ErrorKind::NetworkTimeout);
        } else if err_str.contains("ErrorKind::DatabaseUnavailable") {
            return Some(ErrorKind::DatabaseUnavailable);
        } else if err_str.contains("ErrorKind::TemporaryOverload") {
            return Some(ErrorKind::TemporaryOverload);
        } else if err_str.contains("ErrorKind::InvalidTaskFormat") {
            return Some(ErrorKind::InvalidTaskFormat);
        } else if err_str.contains("ErrorKind::AccountAlreadyExists") {
            return Some(ErrorKind::AccountAlreadyExists);
        } else if err_str.contains("ErrorKind::InsufficientBalance") {
            return Some(ErrorKind::InsufficientBalance);
        } else if err_str.contains("ErrorKind::InvalidOperation") {
            return Some(ErrorKind::InvalidOperation);
        } else if err_str.contains("ErrorKind::ProtobufDecode") {
            return Some(ErrorKind::ProtobufDecode);
        } else if err_str.contains("ErrorKind::TigerBeetle") {
            return Some(ErrorKind::TigerBeetle);
        } else if err_str.contains("ErrorKind::RabbitMQ") {
            return Some(ErrorKind::RabbitMQ);
        }
    }

    None
}

pub fn is_retryable(error: &anyhow::Error) -> bool {
    get_error_kind(error)
        .map(|kind| kind.is_retryable())
        .unwrap_or(false)
}

pub fn retry_delay_ms(error: &anyhow::Error) -> u64 {
    get_error_kind(error)
        .map(|kind| kind.retry_delay_ms())
        .unwrap_or(0)
}

pub fn network_timeout(msg: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{}", msg).context(format!("ErrorKind::{:?}", ErrorKind::NetworkTimeout))
}

pub fn database_unavailable(msg: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{}", msg).context(format!("ErrorKind::{:?}", ErrorKind::DatabaseUnavailable))
}

pub fn temporary_overload(msg: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{}", msg).context(format!("ErrorKind::{:?}", ErrorKind::TemporaryOverload))
}

pub fn invalid_task_format(msg: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{}", msg).context(format!("ErrorKind::{:?}", ErrorKind::InvalidTaskFormat))
}

pub fn account_already_exists(account_id: u128) -> anyhow::Error {
    anyhow!("Account already exists: {}", account_id)
        .context(format!("ErrorKind::{:?}", ErrorKind::AccountAlreadyExists))
}

pub fn insufficient_balance(account_id: u128) -> anyhow::Error {
    anyhow!("Insufficient balance in account: {}", account_id)
        .context(format!("ErrorKind::{:?}", ErrorKind::InsufficientBalance))
}

pub fn invalid_operation(msg: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{}", msg).context(format!("ErrorKind::{:?}", ErrorKind::InvalidOperation))
}
