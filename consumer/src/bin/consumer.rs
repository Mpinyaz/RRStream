use anyhow::Result;
use dotenv::dotenv;
use futures::StreamExt;
use rabbitmq_stream_client::error::StreamCreateError;
use rabbitmq_stream_client::types::{ByteCapacity, OffsetSpecification, ResponseCode};
use rabbitmq_stream_client::Environment;
use rrconsumer::config::Config;
use rrconsumer::responseconsumer::send_task_response;
use rrconsumer::worker::{decode_message, process_task};
use tokio::signal;
use tokio::task;
use tracing::{error, info, warn};

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

        // Send response with the decoded content type and correlation ID
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

    info!("🚀 Starting RabbitMQ Stream Consumer");

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

    // Create or verify request stream exists (for incoming tasks)
    let request_stream = format!("{}_requests", config.stream_name);
    match environment
        .stream_creator()
        .max_length(ByteCapacity::GB(5))
        .max_age(std::time::Duration::from_secs(3600 * 24 * 7))
        .create(&request_stream)
        .await
    {
        Ok(_) => info!("Request stream '{}' created successfully", request_stream),
        Err(StreamCreateError::Create {
            status: ResponseCode::StreamAlreadyExists,
            ..
        }) => {
            info!(
                "Request stream '{}' already exists, using existing stream",
                request_stream
            );
        }
        Err(e) => return Err(e.into()),
    }

    // Create or verify response stream exists (for outgoing responses)
    let response_stream = format!("{}_responses", config.stream_name);
    match environment
        .stream_creator()
        .max_length(ByteCapacity::GB(5))
        .max_age(std::time::Duration::from_secs(3600 * 24 * 7))
        .create(&response_stream)
        .await
    {
        Ok(_) => info!("Response stream '{}' created successfully", response_stream),
        Err(StreamCreateError::Create {
            status: ResponseCode::StreamAlreadyExists,
            ..
        }) => {
            info!(
                "Response stream '{}' already exists, using existing stream",
                response_stream
            );
        }
        Err(e) => return Err(e.into()),
    }

    // Create consumer for incoming tasks (from request stream)
    let mut consumer = environment
        .consumer()
        .offset(OffsetSpecification::Next) // Start from next message
        .build(&request_stream)
        .await?;

    info!("📡 Consumer started for stream: {}", request_stream);

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
