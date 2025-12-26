use tonic_prost_build::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::new();

    // Only types without oneofs
    let types = vec![
        "task.UInt128",
        "task.CreateAccountRequest",
        "task.CreateAccountBatchRequest",
        "task.LookupAccountRequest",
        "task.QueryAccountRequest",
        "task.Account",
        "task.AccountResult",
        "task.CreateTransferRequest",
        "task.CreateTransferBatchRequest",
        "task.LookupTransferRequest",
        "task.QueryTransferRequest",
        "task.Transfer",
        "task.TransferResult",
        "task.PostPendingTransferRequest",
        "task.VoidPendingTransferRequest",
        "task.AccountFlags",
        "task.TransferFlags",
        "task.TransactionStatus",
    ];

    for type_name in types {
        config.type_attribute(type_name, "#[derive(serde::Serialize, serde::Deserialize)]");
    }

    config.compile_protos(&["../proto/task.proto"], &["../proto/"])?;

    Ok(())
}
