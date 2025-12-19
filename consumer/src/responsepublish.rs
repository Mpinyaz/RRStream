use anyhow::Result;
use rabbitmq_stream_client::error::StreamCreateError;
use rabbitmq_stream_client::types::{ByteCapacity, Message as StreamMessage, ResponseCode};
use rabbitmq_stream_client::{Environment, NoDedup, Producer};
use tokio::sync::OnceCell;
use tracing::info;
use uuid::Uuid;

use crate::config::Config;
use crate::task::TaskResponse;
use crate::worker::ContentType;

// 1. Define a Global Static Producer
// We use OnceCell to store the producer so it can be initialized asynchronously.
// We must specify the concrete type <NoDeduplication> instead of "impl Send".
static RESPONSE_PRODUCER: OnceCell<Producer<NoDedup>> = OnceCell::const_new();

/// This helper retrieves the global producer, initializing it only if it doesn't exist yet.
async fn get_global_producer() -> Result<&'static Producer<NoDedup>> {
    RESPONSE_PRODUCER
        .get_or_try_init(init_response_producer)
        .await
}

/// Send a response message using the Global Producer
/// (No need to pass producer as argument anymore)
pub async fn send_response(
    bytes: Vec<u8>,
    content_type: ContentType,
    correlation_id: Option<String>,
) -> Result<()> {
    let producer = get_global_producer().await?;

    info!(
        "Sending response: correlation_id={:?}, content_type={}, size={} bytes",
        correlation_id,
        content_type.as_str(),
        bytes.len()
    );

    let msg = StreamMessage::builder()
        .properties()
        .content_encoding(content_type.as_str())
        .message_id(Uuid::new_v4().to_string())
        .content_type(content_type.as_str())
        .correlation_id(correlation_id.unwrap())
        .message_builder()
        .body(bytes)
        .build();

    producer.send_with_confirm(msg).await?;

    Ok(())
}

pub async fn send_task_response(
    response: &TaskResponse,
    content_type: ContentType,
    correlation_id: Option<String>,
) -> Result<()> {
    use prost::Message as ProstMessage;
    use serde_json::to_vec as json_serialize;

    // Encode based on content type
    let bytes = match content_type {
        ContentType::Json => json_serialize(response)?,
        ContentType::Protobuf => {
            let mut buf = Vec::with_capacity(response.encoded_len());
            response.encode(&mut buf)?;
            buf
        }
    };

    send_response(bytes, content_type, correlation_id).await
}

pub async fn init_response_producer() -> Result<Producer<NoDedup>> {
    let config = Config::from_env()?;

    let environment = Environment::builder()
        .host(&config.host)
        .port(config.port)
        .username(&config.username)
        .password(&config.password)
        .build()
        .await?;

    info!("Connected to RabbitMQ Stream for Producer initialization");

    match environment
        .stream_creator()
        .max_length(ByteCapacity::GB(5))
        .max_age(std::time::Duration::from_secs(3600 * 24 * 7))
        .create(&config.response_stream_name)
        .await
    {
        Ok(_) => info!("Stream '{}' created", config.response_stream_name),

        Err(StreamCreateError::Create {
            status: ResponseCode::StreamAlreadyExists | ResponseCode::PrecoditionFailed,
            ..
        }) => {
            info!(
                "Stream '{}' already exists (or has different properties); continuing",
                config.response_stream_name
            );
        }

        Err(e) => return Err(e.into()),
    }

    let producer = environment
        .producer()
        .build(&config.response_stream_name)
        .await?;

    info!("Global Response Producer initialized");

    Ok(producer)
}
