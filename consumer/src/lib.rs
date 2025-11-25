pub mod config;
pub mod responsepublish;
pub mod worker;
pub mod task {
    tonic::include_proto!("task");
}

use serde::{Deserialize, Serialize};
pub use task::TaskRequest;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub content: String,
    pub timestamp: u64,
}
