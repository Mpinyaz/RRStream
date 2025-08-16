use futures::StreamExt;
use rabbitmq_stream_client::error::StreamCreateError;
use rabbitmq_stream_client::types::{ByteCapacity, OffsetSpecification, ResponseCode};
use rabbitmq_stream_client::Environment;
use tokio::task;

#[tokio::main]
async fn main() -> Result<()> {
    let environment = Environment::builder().build().await?;
}
