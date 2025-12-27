use tonic_prost_build::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::new();

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
    ];

    for type_name in types {
        config.type_attribute(type_name, "#[derive(serde::Serialize, serde::Deserialize)]");
    }
    config.field_attribute("task.TaskRequest.account_batch", "#[serde(default)]");
    config.field_attribute("task.TaskRequest.transfer_batch", "#[serde(default)]");
    config.field_attribute("task.TaskRequest.lookup_ids", "#[serde(default)]");
    config.field_attribute("task.UInt128.low", "#[serde(default)]");
    config.field_attribute("task.UInt128.high", "#[serde(default)]");
    config.compile_protos(&["../proto/task.proto"], &["../proto/"])?;

    Ok(())
}
