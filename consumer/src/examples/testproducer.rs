use anyhow::Result;
use prost::Message as ProstMessage;
use rabbitmq_stream_client::Environment;
use rrconsumer::task::{CreateAccountRequest, Payload, TaskRequest, UInt128};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    // Connect to RabbitMQ
    let environment = Environment::builder()
        .host("localhost")
        .port(5552)
        .username("guest")
        .password("guest")
        .build()
        .await?;

    let producer = environment
        .producer()
        .build("rrmessagebroker_requests")
        .await?;

    // Create account request
    let account_request = CreateAccountRequest {
        id: Some(UInt128 {
            low: 123444,
            high: 0,
        }),
        ledger: 1,
        code: 100,
        user_data_128: Some(UInt128 { low: 0, high: 0 }),
        user_data_64: 0,
        user_data_32: 0,
        flags: 0,
    };

    let task_request = TaskRequest {
        id: Uuid::new_v4().to_string(),
        task_type: "create_account".to_string(),
        content_type: "application/x-protobuf".to_string(),
        payload: Some(Payload {
            operation: Some(rrconsumer::task::payload::Operation::CreateAccount(
                account_request,
            )),
        }),
        created_at: chrono::Utc::now().timestamp(),
        priority: None,
        retry_count: None,
    };

    // Encode to protobuf
    let mut buf = Vec::with_capacity(task_request.encoded_len());
    task_request.encode(&mut buf)?;

    // Send to RabbitMQ
    let msg = rabbitmq_stream_client::types::Message::builder()
        .body(buf)
        .build();

    producer.send_with_confirm(msg).await?;

    println!("✅ Sent create account request");

    Ok(())
}
