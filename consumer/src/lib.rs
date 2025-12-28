pub mod config;
pub mod services;
pub mod worker;
pub mod task {
    tonic::include_proto!("task");
}

use anyhow::{anyhow, Result};
use tigerbeetle_unofficial::error::{CreateAccountError, CreateAccountErrorKind};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    // Transient errors (should retry)
    NetworkTimeout,
    DatabaseUnavailable,
    TemporaryOverload,

    // Permanent errors (should not retry)
    InvalidTaskFormat,
    AccountAlreadyExists,
    InsufficientBalance,
    InvalidOperation,
    ProtobufDecode,
    TigerBeetle,
    RabbitMQ,
}

impl ErrorKind {
    /// Determines if this error should trigger a retry
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ErrorKind::NetworkTimeout
                | ErrorKind::DatabaseUnavailable
                | ErrorKind::TemporaryOverload
        )
    }

    /// Returns the recommended retry delay in milliseconds
    pub fn retry_delay_ms(&self) -> u64 {
        match self {
            ErrorKind::NetworkTimeout => 100,
            ErrorKind::DatabaseUnavailable => 1000,
            ErrorKind::TemporaryOverload => 500,
            _ => 0,
        }
    }
}

/// Extension trait to attach ErrorKind to anyhow::Error
pub trait ErrorKindExt {
    fn with_kind(self, kind: ErrorKind) -> anyhow::Error;
}

impl<T> ErrorKindExt for Result<T, anyhow::Error> {
    fn with_kind(self, kind: ErrorKind) -> anyhow::Error {
        match self {
            Ok(_) => anyhow!("attempted to attach error kind to Ok value"),
            Err(e) => e.context(format!("ErrorKind::{kind:?}")),
        }
    }
}

/// Helper to extract ErrorKind from anyhow::Error chain
pub fn get_error_kind(error: &anyhow::Error) -> Option<ErrorKind> {
    let error_str = format!("{error:?}");

    if error_str.contains("ErrorKind::NetworkTimeout") {
        Some(ErrorKind::NetworkTimeout)
    } else if error_str.contains("ErrorKind::DatabaseUnavailable") {
        Some(ErrorKind::DatabaseUnavailable)
    } else if error_str.contains("ErrorKind::TemporaryOverload") {
        Some(ErrorKind::TemporaryOverload)
    } else if error_str.contains("ErrorKind::InvalidTaskFormat") {
        Some(ErrorKind::InvalidTaskFormat)
    } else if error_str.contains("ErrorKind::AccountAlreadyExists") {
        Some(ErrorKind::AccountAlreadyExists)
    } else if error_str.contains("ErrorKind::InsufficientBalance") {
        Some(ErrorKind::InsufficientBalance)
    } else if error_str.contains("ErrorKind::InvalidOperation") {
        Some(ErrorKind::InvalidOperation)
    } else if error_str.contains("ErrorKind::ProtobufDecode") {
        Some(ErrorKind::ProtobufDecode)
    } else if error_str.contains("ErrorKind::TigerBeetle") {
        Some(ErrorKind::TigerBeetle)
    } else if error_str.contains("ErrorKind::RabbitMQ") {
        Some(ErrorKind::RabbitMQ)
    } else {
        None
    }
}

/// Check if an error is retryable
pub fn is_retryable(error: &anyhow::Error) -> bool {
    get_error_kind(error)
        .map(|kind| kind.is_retryable())
        .unwrap_or(false)
}

/// Get retry delay for an error
pub fn retry_delay_ms(error: &anyhow::Error) -> u64 {
    get_error_kind(error)
        .map(|kind| kind.retry_delay_ms())
        .unwrap_or(0)
}

// Helper functions to create errors with context
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

/* ----------------------------- Error Formatting ----------------------------- */

/// Format CreateAccountError with descriptive message
fn format_account_error(err: &CreateAccountError) -> String {
    match err.kind() {
        CreateAccountErrorKind::Exists => "Account already exists".to_string(),
        CreateAccountErrorKind::ExistsWithDifferentFlags => {
            "Account exists with different flags".to_string()
        }
        CreateAccountErrorKind::ExistsWithDifferentUserData128 => {
            "Account exists with different user_data_128".to_string()
        }
        CreateAccountErrorKind::ExistsWithDifferentUserData64 => {
            "Account exists with different user_data_64".to_string()
        }
        CreateAccountErrorKind::ExistsWithDifferentUserData32 => {
            "Account exists with different user_data_32".to_string()
        }
        CreateAccountErrorKind::ExistsWithDifferentLedger => {
            "Account exists with different ledger".to_string()
        }
        CreateAccountErrorKind::ExistsWithDifferentCode => {
            "Account exists with different code".to_string()
        }
        CreateAccountErrorKind::IdMustNotBeZero => "Account ID must not be zero".to_string(),
        CreateAccountErrorKind::IdMustNotBeIntMax => "Account ID must not be int max".to_string(),
        CreateAccountErrorKind::LedgerMustNotBeZero => "Ledger must not be zero".to_string(),
        CreateAccountErrorKind::CodeMustNotBeZero => "Code must not be zero".to_string(),
        CreateAccountErrorKind::DebitsPendingMustBeZero => {
            "Debits pending must be zero".to_string()
        }
        CreateAccountErrorKind::DebitsPostedMustBeZero => "Debits posted must be zero".to_string(),
        CreateAccountErrorKind::CreditsPendingMustBeZero => {
            "Credits pending must be zero".to_string()
        }
        CreateAccountErrorKind::CreditsPostedMustBeZero => {
            "Credits posted must be zero".to_string()
        }
        CreateAccountErrorKind::FlagsAreMutuallyExclusive => {
            "Flags are mutually exclusive".to_string()
        }
        CreateAccountErrorKind::ReservedFlag => "Reserved flag set".to_string(),
        CreateAccountErrorKind::ReservedField => "Reserved field set".to_string(),
        CreateAccountErrorKind::LinkedEventFailed => "Linked event failed".to_string(),
        CreateAccountErrorKind::LinkedEventChainOpen => "Linked event chain is open".to_string(),
        CreateAccountErrorKind::TimestampMustBeZero => "Timestamp must be zero".to_string(),
        _ => format!("{err}"),
    }
}
