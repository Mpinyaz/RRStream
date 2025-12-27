pub mod config;
pub mod services;
pub mod worker;
pub mod task {
    tonic::include_proto!("task");
}
