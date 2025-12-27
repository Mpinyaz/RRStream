use crate::config::Config;
use crate::task::TaskResponse;
use crate::worker::ContentType;
use anyhow::Result;
use rabbitmq_stream_client::types::{ByteCapacity, Message as StreamMessage, ResponseCode};
use rabbitmq_stream_client::{error::StreamCreateError, NoDedup};
use rabbitmq_stream_client::{Environment, Producer};
use tigerbeetle_unofficial::Client;
use tracing::info;
use uuid::Uuid;

use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

static TB_CLIENT: OnceCell<Arc<Mutex<Client>>> = OnceCell::const_new();

pub async fn get_tb_client() -> Result<Arc<Mutex<Client>>> {
    TB_CLIENT
        .get_or_try_init(|| async {
            let client = init_db_client().await?;
            Ok(Arc::new(Mutex::new(client)))
        })
        .await
        .map(Arc::clone)
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

static RESPONSE_PRODUCER: OnceCell<Producer<NoDedup>> = OnceCell::const_new();

async fn get_response_producer() -> Result<&'static Producer<NoDedup>> {
    RESPONSE_PRODUCER
        .get_or_try_init(init_response_producer)
        .await
}

async fn init_response_producer() -> Result<Producer<NoDedup>> {
    let config = Config::from_env()?;

    let environment = Environment::builder()
        .host(&config.host)
        .port(config.port)
        .username(&config.username)
        .password(&config.password)
        .build()
        .await?;

    let stream = format!("{}_responses", config.stream_name);

    match environment
        .stream_creator()
        .max_length(ByteCapacity::GB(5))
        .max_age(std::time::Duration::from_secs(3600 * 24 * 7))
        .create(&stream)
        .await
    {
        Ok(_) => info!("Stream '{}' created", stream),

        Err(StreamCreateError::Create {
            status: ResponseCode::StreamAlreadyExists | ResponseCode::PrecoditionFailed,
            ..
        }) => {
            info!("Stream '{}' already exists", stream);
        }

        Err(e) => return Err(e.into()),
    }

    let producer = environment.producer().build(&stream).await?;
    info!("Response producer initialized for '{}'", stream);

    Ok(producer)
}

pub async fn send_response(
    bytes: Vec<u8>,
    content_type: ContentType,
    correlation_id: Option<String>,
) -> Result<()> {
    let producer = get_response_producer().await?;

    let mut props = StreamMessage::builder()
        .properties()
        .content_type(content_type.as_str())
        .message_id(Uuid::new_v4().to_string());

    if let Some(cid) = correlation_id {
        props = props.correlation_id(cid);
    }

    let msg = props.message_builder().body(bytes).build();

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

    let bytes = match content_type {
        ContentType::Json => json_serialize(&response)?,
        ContentType::Protobuf => {
            let mut buf = Vec::with_capacity(response.encoded_len());
            response.encode(&mut buf)?;
            buf
        }
    };

    send_response(bytes, content_type, correlation_id).await
}
