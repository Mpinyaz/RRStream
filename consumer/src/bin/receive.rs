use dotenv::dotenv;
use futures::StreamExt;
use rabbitmq_stream_client::error::StreamCreateError;
use rabbitmq_stream_client::types::{ByteCapacity, OffsetSpecification, ResponseCode};
use rabbitmq_stream_client::Environment;
use std::env;
use tokio::task;
use tracing::{error, info, warn};

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
        .max_length(ByteCapacity::GB(2))
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

    let mut consumer = environment.consumer().build(&streamname).await?;
    info!("Consumer started for stream: {}", streamname);
    Ok(())
}
