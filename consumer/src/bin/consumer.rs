use anyhow::Result;
use dotenv::dotenv;
use futures::StreamExt;
use rabbitmq_stream_client::error::StreamCreateError;
use rabbitmq_stream_client::types::{ByteCapacity, OffsetSpecification, ResponseCode};
use rabbitmq_stream_client::Environment;
use rrconsumer::config::Config;
use rrconsumer::worker::{decode_message, process_task};
use tokio::signal;
use tokio::task;
use tracing::{error, info, warn};

use rrconsumer::responsepublish::{init_response_producer, send_task_response};

struct MessageProcessor;

impl MessageProcessor {
    fn new() -> Self {
        Self {}
    }

    async fn process(&self, msg: &rabbitmq_stream_client::types::Message) -> anyhow::Result<()> {
        let decoded = decode_message(msg)?;

        info!(
            "Processing message: task_id={}, correlation_id={:?}",
            decoded.task.id, decoded.correlation_id
        );

        // Process the task
        let response = process_task(msg).await?;

        send_task_response(&response, decoded.content_type, decoded.correlation_id).await?;

        info!("Response sent for task_id={}", response.id);

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🚀 Starting RabbitMQ Stream Consumer with Response Producer");

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

    // Create or verify request stream exists
    match environment
        .stream_creator()
        .max_length(ByteCapacity::GB(5))
        .max_age(std::time::Duration::from_secs(3600 * 24 * 7))
        .create(&config.stream_name)
        .await
    {
        Ok(_) => info!("Stream '{}' created successfully", config.stream_name),
        Err(StreamCreateError::Create {
            status: ResponseCode::StreamAlreadyExists,
            ..
        }) => {
            info!(
                "Stream '{}' already exists, using existing stream",
                config.stream_name
            );
        }
        Err(e) => return Err(e.into()),
    }

    // Create consumer for incoming tasks
    let mut consumer = environment
        .consumer()
        .offset(OffsetSpecification::Next) // Changed to Next for new messages
        .build(&config.response_stream_name)
        .await?;

    info!("📡 Consumer started for stream: {}", config.stream_name);

    match init_response_producer().await {
        Ok(_) => {
            info!("✅ Response producer initialized successfully");
        }
        Err(e) => {
            error!("❌ Failed to start response producer: {}", e);
            return Err(e);
        }
    };

    let handle = consumer.handle();
    let processor = MessageProcessor::new();

    // Spawn consumer task
    let consumer_task = task::spawn(async move {
        info!("Starting message processing loop");
        let mut message_count = 0u64;

        while let Some(delivery) = consumer.next().await {
            match delivery {
                Ok(req) => {
                    message_count += 1;

                    if let Err(e) = processor.process(req.message()).await {
                        error!("Failed to process message: {}", e);
                    }

                    // Log progress every 10 messages
                    if message_count % 10 == 0 {
                        info!("Processed {} messages", message_count);
                    }
                }
                Err(e) => {
                    error!("Error receiving message: {}", e);
                }
            }
        }

        info!(
            "Consumer loop ended. Total messages processed: {}",
            message_count
        );
    });

    info!("✅ System ready - waiting for messages...");
    info!("Press Ctrl+C to shutdown");

    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down gracefully...");
            if let Err(e) = handle.close().await {
                warn!("Error closing consumer: {}", e);
            }
        }
        result = consumer_task => {
            match result {
                Ok(_) => info!("Consumer task completed"),
                Err(e) => error!("Consumer task failed: {}", e),
            }
        }
    }

    info!("🛑 Shutdown complete");
    Ok(())
}
