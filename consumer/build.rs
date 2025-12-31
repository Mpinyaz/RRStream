fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = tonic_prost_build::Config::new();

    let types = vec![
        "task.UInt128",
        "task.CreateAccountRequest",
        "task.CreateTransferRequest",
        "task.TaskRequest",
        "task.TaskResponse",
        "task.Account",
        "task.AccountResult",
        "task.Transfer",
        "task.TransferResult",
        "task.Empty",
    ];

    for type_name in types {
        config.type_attribute(type_name, "#[derive(serde::Serialize, serde::Deserialize)]");
    }

    config.field_attribute("task.TaskRequest.account_batch", "#[serde(default)]");
    config.field_attribute("task.TaskRequest.transfer_batch", "#[serde(default)]");
    config.field_attribute("task.TaskRequest.lookup_ids", "#[serde(default)]");
    config.field_attribute("task.UInt128.low", "#[serde(default)]");
    config.field_attribute("task.UInt128.high", "#[serde(default)]");

    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_with_config(config, &["../proto/task.proto"], &["../proto/"])?;

    Ok(())
}
