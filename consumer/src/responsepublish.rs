use anyhow::Result;
use rabbitmq_stream_client::error::StreamCreateError;
use rabbitmq_stream_client::types::{
    ByteCapacity, Message as StreamMessage, OffsetSpecification, Properties, ResponseCode,
};
use rabbitmq_stream_client::{Environment, Producer};
use tracing::{error, info};
use uuid::Uuid;

use rrconsumer::config::Config;
use rrconsumer::task::TaskResponse;
use rrconsumer::worker::ContentType;

// Then use:
/// Send a response message to the reply stream
/// with correct content-type and correlation-id.
pub async fn send_response<T>(
    producer: &Producer<T>,
    bytes: Vec<u8>,
    content_type: ContentType,
    correlation_id: Option<String>,
) -> Result<()>
where
    T: Send + Sync,
{
    let mut props = Properties::default();

    // Set content type
    props.content_type = Some(content_type.as_str().into());

    // Set correlation_id from request
    props.correlation_id = correlation_id.clone();

    // Unique message ID for tracing
    props.message_id = Some(Uuid::new_v4().to_string().into());

    info!(
        "Sending response: correlation_id={:?}, content_type={}, size={} bytes",
        correlation_id,
        content_type.as_str(),
        bytes.len()
    );

    // Build and send message
    let msg = StreamMessage::builder()
        .body(bytes)
        .properties(props)
        .build();

    producer.send(msg).await?;

    Ok(())
}

/// Helper to encode and send a TaskResponse
pub async fn send_task_response<T>(
    producer: &Producer<T>,
    response: &TaskResponse,
    content_type: ContentType,
    correlation_id: Option<String>,
) -> Result<()>
where
    T: Send + Sync,
{
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

    // Send the encoded response
    send_response(producer, bytes, content_type, correlation_id).await
}
/// Initialize the response stream producer
pub async fn start_response_producer() -> Result<Producer<impl Send + Sync>> {
    // Load configuration
    let config = Config::from_env()?;
    info!("Configuration loaded successfully");

    // Create environment
    let environment = Environment::builder()
        .host(&config.host)
        .port(config.port)
        .username(&config.username)
        .password(&config.password)
        .build()
        .await?;

    info!(
        "Connected to RabbitMQ Stream at {}:{}",
        config.host, config.port
    );

    // Create or verify stream exists
    match environment
        .stream_creator()
        .max_length(ByteCapacity::GB(5))
        .max_age(std::time::Duration::from_secs(3600 * 24 * 7))
        .create(&config.response_stream_name)
        .await
    {
        Ok(_) => info!(
            "Stream '{}' created successfully",
            config.response_stream_name
        ),
        Err(StreamCreateError::Create {
            status: ResponseCode::StreamAlreadyExists,
            ..
        }) => {
            info!(
                "Stream '{}' already exists, using existing stream",
                config.response_stream_name
            );
        }
        Err(e) => return Err(e.into()),
    }

    // Create producer for responses
    let producer = environment
        .producer()
        .name("response_producer")
        .build(&config.response_stream_name)
        .await?;

    info!("Response producer created successfully");

    Ok(producer)
}
