use anyhow::anyhow;
use std::fmt;
pub mod config;
pub mod services;
pub mod worker;
pub mod task {
    tonic::include_proto!("task");
}
pub struct AppError {
    kind: ErrorKind,
    message: String,
    status: ErrorStatus,
    operation: &'static str,
    context: Vec<(&'static str, String)>,
    source: Option<anyhow::Error>,
}

#[derive(Debug, Clone, Copy)]
pub enum ErrorStatus {
    Permanent,
    Temporary,
    Persistent,
}

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

    fn map_status(self) -> ErrorStatus {
        match self {
            ErrorKind::TemporaryOverload
            | ErrorKind::DatabaseUnavailable
            | ErrorKind::NetworkTimeout => ErrorStatus::Temporary,
            _ => ErrorStatus::Permanent,
        }
    }
}

impl AppError {
    pub fn new(kind: ErrorKind, operation: &'static str) -> Self {
        Self {
            kind,
            message: String::new(),
            status: kind.map_status(),
            operation,
            context: Vec::new(),
            source: None,
        }
    }

    pub fn with_message(kind: ErrorKind, operation: &'static str, msg: impl Into<String>) -> Self {
        Self {
            kind,
            message: msg.into(),
            status: kind.map_status(),
            operation,
            context: Vec::new(),
            source: None,
        }
    }

    pub fn operation(mut self, operation: &'static str) -> Self {
        self.operation = operation;
        self
    }

    pub fn kind(mut self, kind: ErrorKind) -> Self {
        self.kind = kind;
        self.status = kind.map_status();
        self
    }

    pub fn context(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.context.push((key, value.into()));
        self
    }

    pub fn with_context(mut self, context: Vec<(&'static str, String)>) -> Self {
        self.context.extend(context);
        self
    }

    pub fn message(mut self, msg: impl Into<String>) -> Self {
        self.message = msg.into();
        self
    }

    pub fn status(mut self, status: ErrorStatus) -> Self {
        self.status = status;
        self
    }

    pub fn source(mut self, source: anyhow::Error) -> Self {
        self.source = Some(source);
        self
    }

    // Getter methods
    pub fn get_kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn get_status(&self) -> ErrorStatus {
        self.status
    }

    pub fn get_operation(&self) -> &'static str {
        self.operation
    }

    pub fn get_message(&self) -> &str {
        &self.message
    }

    pub fn get_context(&self) -> &[(&'static str, String)] {
        &self.context
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self.status, ErrorStatus::Temporary)
    }

    pub fn retry_delay_ms(&self) -> u64 {
        self.kind.retry_delay_ms()
    }

    // Convert to anyhow::Error
    pub fn into_anyhow(self) -> anyhow::Error {
        let mut err = if let Some(source) = self.source {
            source.context(self.message.clone())
        } else {
            anyhow!("{}", self.message)
        };

        for (key, value) in self.context {
            err = err.context(format!("{key}={value}"));
        }

        err = err.context(format!("operation={}", self.operation));

        err.context(format!("ErrorKind::{:?}", self.kind))
    }
}

// Implement Display for AppError
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.operation, self.kind, self.message)?;

        if !self.context.is_empty() {
            write!(f, " (")?;
            for (i, (key, value)) in self.context.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{key}={value}")?;
            }
            write!(f, ")")?;
        }

        if let Some(source) = &self.source {
            write!(f, " | caused by: {source}")?;
        }

        Ok(())
    }
}

impl fmt::Debug for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppError")
            .field("kind", &self.kind)
            .field("status", &self.status)
            .field("operation", &self.operation)
            .field("message", &self.message)
            .field("context", &self.context)
            .field("source", &self.source)
            .finish()
    }
}

// Implement std::error::Error for AppError
impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

// Extension trait for adding ErrorKind to anyhow::Error
pub trait ErrorKindExt {
    fn with_kind(self, kind: ErrorKind) -> anyhow::Error;
}

impl ErrorKindExt for anyhow::Error {
    fn with_kind(self, kind: ErrorKind) -> anyhow::Error {
        self.context(format!("ErrorKind::{kind:?}"))
    }
}

// Helper functions to extract error information from anyhow::Error
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
        } else if err_str.contains("ErrorKind::InvalidInput") {
            return Some(ErrorKind::InvalidInput);
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

// Convenience constructor functions
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

pub fn protobuf_decode(msg: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{}", msg).context(format!("ErrorKind::{:?}", ErrorKind::ProtobufDecode))
}

pub fn tigerbeetle_error(msg: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{}", msg).context(format!("ErrorKind::{:?}", ErrorKind::TigerBeetle))
}

pub fn rabbitmq_error(msg: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{}", msg).context(format!("ErrorKind::{:?}", ErrorKind::RabbitMQ))
}

pub fn invalid_input(msg: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{}", msg).context(format!("ErrorKind::{:?}", ErrorKind::InvalidInput))
}
