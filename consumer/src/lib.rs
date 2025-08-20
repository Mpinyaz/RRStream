pub mod processes;
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;
pub async fn process_message(message: &str, offset: u64) -> Result<(), anyhow::Error> {
    // Simulate some processing time
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Try to parse as JSON if possible
    match serde_json::from_str::<serde_json::Value>(message) {
        Ok(json_value) => {
            info!(
                "Processed JSON message at offset {}: {:#}",
                offset, json_value
            );
        }
        Err(_) => {
            info!("Processed text message at offset {}: {}", offset, message);
        }
    }

    Ok(())
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub task_type: String,
    pub payload: serde_json::Value,
    pub retry_count: Option<u32>,
}
