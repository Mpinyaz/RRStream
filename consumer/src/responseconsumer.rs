use crate::config::Config;
use crate::models::SerializableTaskResponse;
use crate::task::TaskResponse;
use crate::worker::ContentType;
use anyhow::Result;
use rabbitmq_stream_client::error::StreamCreateError;
use rabbitmq_stream_client::types::{ByteCapacity, Message as StreamMessage, ResponseCode};
use rabbitmq_stream_client::{Consumer, Environment};
use tigerbeetle_unofficial::Client;
use tracing::info;
use uuid::Uuid;

use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

static TB_CLIENT: OnceCell<Arc<Mutex<Client>>> = OnceCell::const_new();
static RESPONSE_CONSUMER: OnceCell<Consumer> = OnceCell::const_new();

pub async fn get_tb_client() -> Result<Arc<Mutex<Client>>> {
    TB_CLIENT
        .get_or_try_init(|| async {
            let client = init_db_client().await?;
            Ok(Arc::new(Mutex::new(client)))
        })
        .await
        .map(|c| Arc::clone(c))
}

async fn init_db_client() -> Result<Client> {
    let config = Config::from_env()?;
    let client = Client::new(0, config.tb_address.as_str())
        .map_err(|e| anyhow::anyhow!("Failed to connect to TigerBeetle: {:?}", e))?;

    info!(
        "TigerBeetle client initialized with address: {}",
        config.tb_address
    );

    Ok(client)
}

async fn get_response_consumer() -> Result<&'static Consumer> {
    RESPONSE_CONSUMER
        .get_or_try_init(init_response_consumer)
        .await
}

/// Send a response message using the Global Producer
/// (No need to pass producer as argument anymore)
pub async fn send_response(
    bytes: Vec<u8>,
    content_type: ContentType,
    correlation_id: Option<String>,
) -> Result<()> {
    let _consumer = get_response_consumer().await?;

    info!(
        "Sending response: correlation_id={:?}, content_type={}, size={} bytes",
        correlation_id,
        content_type.as_str(),
        bytes.len()
    );

    let _msg = StreamMessage::builder()
        .properties()
        .content_encoding(content_type.as_str())
        .message_id(Uuid::new_v4().to_string())
        .content_type(content_type.as_str())
        // .correlation_id(correlation_id.unwrap())
        .message_builder()
        .body(bytes)
        .build();

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
        ContentType::Json => {
            // Convert to serializable version for JSON
            let serializable: SerializableTaskResponse = response.clone().into();
            json_serialize(&serializable)?
        }
        ContentType::Protobuf => {
            let mut buf = Vec::with_capacity(response.encoded_len());
            response.encode(&mut buf)?;
            buf
        }
    };

    send_response(bytes, content_type, correlation_id).await
}

pub async fn init_response_consumer() -> Result<Consumer> {
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
        .create(&config.stream_name)
        .await
    {
        Ok(_) => info!("Stream '{}' created", config.stream_name),

        Err(StreamCreateError::Create {
            status: ResponseCode::StreamAlreadyExists | ResponseCode::PrecoditionFailed,
            ..
        }) => {
            info!(
                "Stream '{}' already exists (or has different properties); continuing",
                config.stream_name
            );
        }

        Err(e) => return Err(e.into()),
    }

    let producer = environment.consumer().build(&config.stream_name).await?;

    info!("Response Consumer initialized");

    Ok(producer)
}
