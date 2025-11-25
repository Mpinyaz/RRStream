use tonic_prost_build::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::new();

    config.type_attribute(
        "task.TaskRequest",
        "#[derive(serde::Serialize, serde::Deserialize)]",
    );
    config.type_attribute(
        "task.TaskResponse",
        "#[derive(serde::Serialize, serde::Deserialize)]",
    );

    // Compile the proto file
    config.compile_protos(&["../proto/task.proto"], &["../proto/"])?;
    Ok(())
}
