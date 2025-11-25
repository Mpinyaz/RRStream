use crate::task::{TaskRequest, TaskResponse};
use anyhow::Result;
use prost::Message as ProstMessage;
use rabbitmq_stream_client::types::Message as StreamMessage;
use serde_json::from_slice;
use serde_json::to_vec as json_serialize;
use std::str::FromStr;
use tracing::{debug, info, warn};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    Json,
    Protobuf,
}

impl FromStr for ContentType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "application/json" => Ok(Self::Json),
            "application/x-protobuf" => Ok(Self::Protobuf),
            _ => Ok(Self::Protobuf), // Default to protobuf
        }
    }
}

impl ContentType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Json => "application/json",
            Self::Protobuf => "application/x-protobuf",
        }
    }
}

pub struct DecodedMessage {
    pub task: TaskRequest,
    pub content_type: ContentType,
    pub correlation_id: Option<String>,
}

impl DecodedMessage {
    pub fn new(
        task: TaskRequest,
        content_type: ContentType,
        correlation_id: Option<String>,
    ) -> Self {
        Self {
            task,
            content_type,
            correlation_id,
        }
    }
}

pub fn decode_message(msg: &StreamMessage) -> Result<DecodedMessage> {
    let bytes = msg
        .data()
        .ok_or_else(|| anyhow::anyhow!("Message contains no data"))?;

    let properties = msg.properties();

    let content_type_string = properties
        .and_then(|p| p.content_type.as_ref())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "application/x-protobuf".to_string());

    let correlation_id = properties.and_then(|p| {
        p.correlation_id.as_ref().map(|cid| {
            // correlation_id is a String in Properties, so this should work
            cid.clone()
        })
    });
    let content_type = ContentType::from_str(&content_type_string).unwrap_or(ContentType::Protobuf);

    debug!(
        "Decoding message: content_type={}, size={} bytes, correlation_id={:?}",
        content_type.as_str(),
        bytes.len(),
        correlation_id
    );

    let task = match content_type {
        ContentType::Json => decode_json(bytes)?,
        ContentType::Protobuf => decode_protobuf(bytes)?,
    };

    Ok(DecodedMessage::new(task, content_type, correlation_id))
}

fn decode_json(bytes: &[u8]) -> Result<TaskRequest> {
    from_slice(bytes).map_err(|e| anyhow::anyhow!(e))
}

fn decode_protobuf(bytes: &[u8]) -> Result<TaskRequest> {
    TaskRequest::decode(bytes).map_err(|e| anyhow::anyhow!(e))
}

/// Processes a decoded message and returns a TaskResponse
pub async fn process_task(msg: &StreamMessage) -> Result<TaskResponse> {
    let decoded = decode_message(msg)?;

    info!(
        "Processing task: id={}, type={}, format={}, correlation_id={:?}",
        decoded.task.id,
        decoded.task.task_type,
        decoded.content_type.as_str(),
        decoded.correlation_id
    );

    debug!("Task payload: {:?}", decoded.task.payload);

    // Route based on task type
    let result = match decoded.task.task_type.as_str() {
        "email_notification" => handle_email_notification(&decoded.task).await,
        "process_order" => handle_process_order(&decoded.task).await,
        "generate_report" => handle_generate_report(&decoded.task).await,
        _ => handle_default(&decoded.task).await,
    };

    match result {
        Ok(message) => {
            info!("Task {} processed successfully", decoded.task.id);
            Ok(TaskResponse {
                id: decoded.task.id.clone(),
                task_type: decoded.task.task_type.clone(),
                success: true,
                message,
            })
        }
        Err(e) => {
            warn!("Task {} processing failed: {}", decoded.task.id, e);
            Ok(TaskResponse {
                id: decoded.task.id.clone(),
                task_type: decoded.task.task_type.clone(),
                success: false,
                message: format!("Processing failed: {e}"),
            })
        }
    }
}

async fn handle_email_notification(task: &TaskRequest) -> Result<String> {
    info!("Sending email notification for task {}", task.id);
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    Ok(format!("Email notification sent for task {}", task.id))
}

async fn handle_process_order(task: &TaskRequest) -> Result<String> {
    info!("Processing order for task {}", task.id);
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
    Ok(format!("Order processed for task {}", task.id))
}

async fn handle_generate_report(task: &TaskRequest) -> Result<String> {
    info!("Generating report for task {}", task.id);
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    Ok(format!("Report generated for task {}", task.id))
}

async fn handle_default(task: &TaskRequest) -> Result<String> {
    info!("Handling generic task {}", task.id);
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    Ok(format!("Task {} processed", task.id))
}

pub fn encode_response(resp: &TaskResponse, content_type: ContentType) -> Result<Vec<u8>> {
    match content_type {
        ContentType::Json => Ok(json_serialize(resp)?),
        ContentType::Protobuf => {
            let mut buf = Vec::with_capacity(resp.encoded_len());
            resp.encode(&mut buf)?;
            Ok(buf)
        }
    }
}
