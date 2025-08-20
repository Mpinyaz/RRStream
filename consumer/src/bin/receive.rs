use dotenv::dotenv;
use futures::StreamExt;
use rabbitmq_stream_client::error::StreamCreateError;
use rabbitmq_stream_client::types::{ByteCapacity, OffsetSpecification, ResponseCode};
use rabbitmq_stream_client::Environment;
use std::env;
use tokio::signal;
use tokio::task;
use tracing::{error, info, warn};
use RRconsumer::process_message;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenv().ok();

    let host = env::var("RABBITMQ_ADVERTISED_HOST").expect("RABBITMQ_ADVERTISED_HOST must be set");
    let port: u16 = env::var("RABBITMQ_STREAM_PORT")
        .expect("RABBITMQ_STREAM_PORT must be set")
        .parse()
        .expect("RABBITMQ_STREAM_PORT must be a valid number");
    let username = env::var("RABBITMQ_DEFAULT_USER").expect("RABBITMQ_DEFAULT_USER must be set");
    let password = env::var("RABBITMQ_DEFAULT_PASS").expect("RABBITMQ_DEFAULT_PASS must be set");
    let streamname = env::var("RABBITMQ_STREAM_NAME").expect("RABBITMQ_STREAM_NAME must be set");

    tracing_subscriber::fmt::init();
    let environment = Environment::builder()
        .host(&host)
        .port(port)
        .username(&username)
        .password(&password)
        .build()
        .await?;

    info!("Connected to RabbitMQ Stream");

    match environment
        .stream_creator()
        .max_length(ByteCapacity::GB(5))
        .max_age(std::time::Duration::from_secs(3600 * 24 * 7))
        .create(&streamname)
        .await
    {
        Ok(_) => info!("Stream '{}' created successfully", streamname),
        Err(StreamCreateError::Create {
            status: ResponseCode::StreamAlreadyExists,
            ..
        }) => {
            warn!("Stream '{}' already exists", streamname);
        }
        Err(e) => return Err(e.into()),
    }

    let mut consumer = environment
        .consumer()
        .offset(OffsetSpecification::First)
        .build(&streamname)
        .await?;
    info!("Consumer started for stream: {}", streamname);
    let handle = consumer.handle();

    let consumer_task = task::spawn(async move {
        info!("Starting message processing loop");

        while let Some(delivery) = consumer.next().await {
            match delivery {
                Ok(delivery_result) => {
                    let message_data = delivery_result
                        .message()
                        .data()
                        .map(|data| String::from_utf8_lossy(data).to_string())
                        .unwrap_or_else(|| "No data".to_string());

                    info!(
                        "Received message: {} (offset: {})",
                        message_data,
                        delivery_result.offset()
                    );

                    if let Err(e) = process_message(&message_data, delivery_result.offset()).await {
                        error!("Failed to process message: {}", e);
                    }
                }
                Err(e) => {
                    error!("Error receiving message: {}", e);
                }
            }
        }

        info!("Consumer loop ended");
    });

    tokio::select! {
        _ = signal::ctrl_c()=> {

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
    Ok(())
}
