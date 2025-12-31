use anyhow::Result;
use dotenv::dotenv;
use futures::StreamExt;
use rabbitmq_stream_client::error::StreamCreateError;
use rabbitmq_stream_client::types::{
    ByteCapacity, Message as StreamMessage, OffsetSpecification, ResponseCode,
};
use rabbitmq_stream_client::Environment;
use rrconsumer::config::Config;
use rrconsumer::services::send_task_response;
use rrconsumer::worker::{decode_message, process_task};
use tokio::signal;
use tokio::task;
use tracing::{error, info, warn};

struct MessageProcessor;

impl MessageProcessor {
    fn new() -> Self {
        Self {}
    }

    async fn process(&self, msg: &StreamMessage) -> anyhow::Result<()> {
        let decoded = decode_message(msg)?;
        let correlation_id = decoded.correlation_id.clone().unwrap();

        info!(
            "Processing incoming task: id={}, correlation_id={}",
            decoded.task.id, correlation_id
        );

        let response_msg = process_task(msg).await?;

        // Send the response with the same correlation ID
        send_task_response(
            &response_msg,
            decoded.content_type,
            Some(correlation_id.clone()),
        )
        .await?;

        info!(
            "Response successfully sent: id={}, correlation_id={}",
            decoded.task.id, correlation_id
        );

        Ok(())
    }
}

async fn ensure_streams(env: &Environment, base: &str) -> Result<()> {
    let streams = [format!("{base}_requests"), format!("{base}_responses")];

    for stream in streams {
        match env
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
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🚀 Starting RabbitMQ Stream Consumer");

    let config = Config::from_env()?;
    info!("Configuration loaded");

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

    // 🔒 Ensure streams exist ONCE
    ensure_streams(&environment, &config.stream_name).await?;

    let request_stream = format!("{}_requests", config.stream_name);
    let consumer_name = "tigerbeetle_consumer";

    let mut consumer = environment
        .consumer()
        .name(consumer_name) // Enable offset tracking
        .offset(OffsetSpecification::Next)
        .build(&request_stream)
        .await?;

    info!(
        "📡 Consumer '{}' started on stream: {}",
        consumer_name, request_stream
    );

    let handle = consumer.handle();
    let processor = MessageProcessor::new();

    let consumer_task = task::spawn(async move {
        let mut message_count = 0u64;

        while let Some(delivery) = consumer.next().await {
            match delivery {
                Ok(req) => {
                    message_count += 1;

                    if let Err(e) = processor.process(req.message()).await {
                        error!("Failed to process message: {}", e);
                    }

                    if message_count % 10 == 0 {
                        info!("Processed {} messages", message_count);
                    }
                }
                Err(e) => error!("Error receiving message: {}", e),
            }
        }

        info!("Consumer stopped. Total processed: {}", message_count);
    });

    info!("✅ Ready — waiting for messages");
    info!("Press Ctrl+C to shutdown");

    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Shutdown requested");
            if let Err(e) = handle.close().await {
                warn!("Error closing consumer: {}", e);
            }
        }
        res = consumer_task => {
            if let Err(e) = res {
                error!("Consumer task failed: {}", e);
            }
        }
    }

    info!("🛑 Shutdown complete");
    Ok(())
}
