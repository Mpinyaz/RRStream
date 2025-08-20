use tracing::info;
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
