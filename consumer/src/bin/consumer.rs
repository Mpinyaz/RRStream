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
use rrconsumer::{AppError, ErrorKind};
use tokio::signal;
use tokio::task;
use tracing::{error, info, warn};

struct MessageProcessor;

impl MessageProcessor {
    fn new() -> Self {
        Self {}
    }

    async fn process(&self, msg: &StreamMessage) -> anyhow::Result<()> {
        // Decode message with proper error handling
        let decoded = decode_message(msg).map_err(|e| {
            let app_err = AppError::new(ErrorKind::RabbitMQ, "process_message")
                .message(format!("Failed to decode message: {}", e))
                .source(e);
            error!("{}", app_err);
            app_err.into_anyhow()
        })?;

        let correlation_id = decoded
            .correlation_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        info!(
            "Processing incoming task: id={}, correlation_id={}",
            decoded.task.id, correlation_id
        );

        // Process task with error handling
        let response_msg = process_task(msg).await.map_err(|e| {
            let app_err = AppError::new(ErrorKind::InvalidOperation, "process_message")
                .message(format!("Failed to process task: {}", e))
                .context("task_id", decoded.task.id.clone())
                .context("correlation_id", correlation_id.clone())
                .source(e);
            error!("{}", app_err);
            app_err.into_anyhow()
        })?;

        // Send the response with the same correlation ID
        send_task_response(
            &response_msg,
            decoded.content_type,
            Some(correlation_id.clone()),
        )
        .await
        .map_err(|e| {
            let app_err = AppError::new(ErrorKind::RabbitMQ, "process_message")
                .message(format!("Failed to send response: {}", e))
                .context("task_id", decoded.task.id.clone())
                .context("correlation_id", correlation_id.clone())
                .source(e);
            error!("{}", app_err);
            app_err.into_anyhow()
        })?;

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
            Ok(_) => info!("Stream '{}' created successfully", stream),
            Err(StreamCreateError::Create {
                status: ResponseCode::StreamAlreadyExists | ResponseCode::PrecoditionFailed,
                ..
            }) => {
                info!("Stream '{}' already exists", stream);
            }
            Err(e) => {
                let app_err = AppError::new(ErrorKind::RabbitMQ, "ensure_streams")
                    .message(format!("Failed to create stream '{}': {}", stream, e))
                    .context("stream_name", stream.clone());
                error!("{}", app_err);
                return Err(app_err.into_anyhow());
            }
        }
    }

    Ok(())
}

async fn initialize_environment(config: &Config) -> Result<Environment> {
    Environment::builder()
        .host(&config.host)
        .port(config.port)
        .username(&config.username)
        .password(&config.password)
        .build()
        .await
        .map_err(|e| {
            AppError::new(ErrorKind::RabbitMQ, "initialize_environment")
                .message(format!("Failed to connect to RabbitMQ: {}", e))
                .context("host", config.host.clone())
                .context("port", config.port.to_string())
                .into_anyhow()
        })
}

async fn create_consumer(
    environment: &Environment,
    request_stream: &str,
    consumer_name: &str,
) -> Result<rabbitmq_stream_client::Consumer> {
    environment
        .consumer()
        .name(consumer_name)
        .offset(OffsetSpecification::Next)
        .build(request_stream)
        .await
        .map_err(|e| {
            AppError::new(ErrorKind::RabbitMQ, "create_consumer")
                .message(format!("Failed to create consumer: {}", e))
                .context("stream_name", request_stream.to_string())
                .context("consumer_name", consumer_name.to_string())
                .into_anyhow()
        })
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv().ok();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🚀 Starting RabbitMQ Stream Consumer");

    // Load configuration
    let config = Config::from_env().map_err(|e| {
        AppError::new(ErrorKind::InvalidInput, "main")
            .message(format!("Failed to load configuration: {}", e))
            .source(e)
            .into_anyhow()
    })?;
    info!("Configuration loaded successfully");

    // Connect to RabbitMQ
    let environment = initialize_environment(&config).await?;
    info!(
        "Connected to RabbitMQ Stream at {}:{}",
        config.host, config.port
    );

    // Ensure streams exist
    ensure_streams(&environment, &config.stream_name).await?;

    let request_stream = format!("{}_requests", config.stream_name);
    let consumer_name = "tigerbeetle_consumer";

    // Create consumer
    let mut consumer = create_consumer(&environment, &request_stream, consumer_name).await?;

    info!(
        "📡 Consumer '{}' started on stream: {}",
        consumer_name, request_stream
    );

    let handle = consumer.handle();
    let processor = MessageProcessor::new();

    let consumer_task = task::spawn(async move {
        let mut message_count = 0u64;
        let mut error_count = 0u64;

        while let Some(delivery) = consumer.next().await {
            match delivery {
                Ok(req) => {
                    message_count += 1;

                    match processor.process(req.message()).await {
                        Ok(_) => {
                            if message_count % 10 == 0 {
                                info!(
                                    "Processed {} messages (errors: {})",
                                    message_count, error_count
                                );
                            }
                        }
                        Err(e) => {
                            error_count += 1;
                            error!("Failed to process message {}: {}", message_count, e);

                            // Check if error is retryable
                            if rrconsumer::is_retryable(&e) {
                                let delay = rrconsumer::retry_delay_ms(&e);
                                warn!(
                                    "Error is retryable, would retry after {}ms (not implemented yet)",
                                    delay
                                );
                            } else {
                                warn!("Error is permanent, skipping message");
                            }
                        }
                    }
                }
                Err(e) => {
                    error_count += 1;
                    let app_err = AppError::new(ErrorKind::RabbitMQ, "consumer_loop")
                        .message(format!("Error receiving message: {}", e));
                    error!("{}", app_err);
                }
            }
        }

        info!(
            "Consumer stopped. Total processed: {}, Errors: {}",
            message_count, error_count
        );
    });

    info!("✅ Ready — waiting for messages");
    info!("Press Ctrl+C to shutdown");

    // Wait for shutdown signal or consumer task completion
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Shutdown signal received (Ctrl+C)");
            if let Err(e) = handle.close().await {
                let app_err = AppError::new(ErrorKind::RabbitMQ, "shutdown")
                    .message(format!("Error closing consumer: {}", e));
                warn!("{}", app_err);
            } else {
                info!("Consumer closed successfully");
            }
        }
        res = consumer_task => {
            match res {
                Ok(_) => {
                    info!("Consumer task completed normally");
                }
                Err(e) => {
                    let app_err = AppError::new(ErrorKind::InvalidOperation, "main")
                        .message(format!("Consumer task panicked: {}", e));
                    error!("{}", app_err);
                    return Err(app_err.into_anyhow());
                }
            }
        }
    }

    info!("🛑 Shutdown complete");
    Ok(())
}
