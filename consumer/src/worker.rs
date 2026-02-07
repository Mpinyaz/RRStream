use crate::ErrorKind;
use crate::{invalid_task_format, services::get_tb_client};
use crate::{
    task::{
        Account, AccountResult, TaskRequest, TaskResponse, TaskType, Transfer, TransferResult,
        UInt128,
    },
    AppError,
};
use anyhow::Result;
use prost::Message as ProstMessage;
use rabbitmq_stream_client::types::Message as StreamMessage;
use serde_json::to_vec as json_serialize;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tigerbeetle_unofficial::error::{CreateAccountsError, CreateTransfersError};
use tigerbeetle_unofficial::{Account as TBAccount, Transfer as TBTransfer};
use tracing::{error, info};
use uuid::Uuid;

fn system_time_to_u64(ts: SystemTime) -> u64 {
    ts.duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64
}

pub fn uint128_to_proto(value: u128) -> UInt128 {
    info!("Converting u128 {} to UInt128", value);
    UInt128 {
        low: value as u64,
        high: (value >> 64) as u64,
    }
}

pub fn proto_to_uint128(v: &UInt128) -> u128 {
    let val = ((v.high as u128) << 64) | v.low as u128;
    info!(
        "Converting UInt128 {{high={}, low={}}} to u128 {}",
        v.high, v.low, val
    );
    val
}

#[derive(Debug, Clone, Copy)]
pub enum ContentType {
    Json,
    Protobuf,
}

impl FromStr for ContentType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "application/json" => Self::Json,
            "application/x-protobuf" => Self::Protobuf,
            _ => Self::Protobuf,
        })
    }
}

impl ContentType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Json => "application/json",
            Self::Protobuf => "application/x-protobuf",
        }
    }
}

pub struct DecodedMessage {
    pub task: TaskRequest,
    pub content_type: ContentType,
    pub correlation_id: String,
}

impl DecodedMessage {
    pub fn new(task: TaskRequest, content_type: ContentType, correlation_id: String) -> Self {
        info!("DecodedMessage created for task_id={}", task.id);
        Self {
            task,
            content_type,
            correlation_id,
        }
    }
}

pub fn decode_message(msg: &StreamMessage) -> Result<DecodedMessage> {
    info!("Decoding incoming message");
    let bytes = msg.data().ok_or_else(|| {
        AppError::new(ErrorKind::RabbitMQ, "decode_message")
            .message("Message contains no data")
            .into_anyhow()
    })?;

    let properties = msg.properties();
    let content_type_string = properties
        .and_then(|p| p.content_type.as_ref())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "application/x-protobuf".to_string());

    let correlation_id = match properties
        .unwrap()
        .correlation_id
        .as_ref()
        .and_then(|msg_id| {
            let cloned = msg_id.clone();
            cloned.try_into().ok()
        }) {
        Some(id) => id,
        None => {
            return Err(
                AppError::new(ErrorKind::InvalidTaskFormat, "decode_message")
                    .message("correlation_id must be provided")
                    .into_anyhow(),
            );
        }
    };

    let content_type = ContentType::from_str(&content_type_string).unwrap_or(ContentType::Protobuf);

    info!(
        "Message info: content_type={}, size={} bytes, correlation_id={:?}",
        content_type.as_str(),
        bytes.len(),
        correlation_id
    );

    let task = match content_type {
        ContentType::Json => decode_json(bytes)?,
        ContentType::Protobuf => decode_protobuf(bytes)?,
    };

    Ok(DecodedMessage::new(task, content_type, correlation_id))
}

fn decode_protobuf(bytes: &[u8]) -> Result<TaskRequest> {
    info!("Decoding Protobuf message of {} bytes", bytes.len());
    TaskRequest::decode(bytes).map_err(|e| {
        AppError::new(ErrorKind::ProtobufDecode, "decode_protobuf")
            .message(format!("Failed to decode protobuf: {}", e))
            .context("bytes_len", bytes.len().to_string())
            .into_anyhow()
    })
}

fn decode_json(bytes: &[u8]) -> Result<TaskRequest> {
    match serde_json::from_slice::<TaskRequest>(bytes) {
        Ok(task) => {
            info!("✅ JSON decoded successfully: task_id={}", task.id);
            Ok(task)
        }
        Err(e) => Err(AppError::new(ErrorKind::InvalidTaskFormat, "decode_json")
            .message(format!("Failed to decode JSON: {}", e))
            .context("bytes_len", bytes.len().to_string())
            .into_anyhow()),
    }
}

pub async fn process_task(msg: &StreamMessage) -> Result<TaskResponse> {
    info!("Starting process_task");
    let decoded = decode_message(msg)?;
    info!(
        "Processing task id={} type={:?}",
        decoded.task.id, decoded.task.task_type
    );

    let task_type = TaskType::try_from(decoded.task.task_type).map_err(|_| {
        AppError::new(ErrorKind::InvalidTaskFormat, "process_task")
            .message("Invalid task_type value")
            .context("task_type", decoded.task.task_type.to_string())
            .into_anyhow()
    })?;

    let response = match task_type {
        TaskType::CreateAccount => handle_create_account(&decoded.task).await,
        TaskType::BatchAccounts => handle_create_account_batch(&decoded.task).await,
        TaskType::LookupAccounts => handle_lookup_accounts(&decoded.task).await,
        TaskType::CreateTransfer => handle_create_transfer(&decoded.task).await,
        TaskType::BatchTransfers => handle_create_transfer_batch(&decoded.task).await,
        TaskType::LookupTransfers => handle_lookup_transfers(&decoded.task).await,
        TaskType::Unknown => Err(AppError::new(ErrorKind::InvalidTaskFormat, "process_task")
            .message("task_type UNKNOWN is invalid")
            .into_anyhow()),
    };

    info!("Finished process_task for task_id={}", decoded.task.id);
    response
}

async fn handle_create_account(task: &TaskRequest) -> Result<TaskResponse> {
    info!("handle_create_account called for task_id={}", task.id);
    let id = proto_to_uint128(
        task.account_id
            .as_ref()
            .ok_or_else(|| invalid_task_format("Missing account_id"))?,
    );
    info!("Account id decoded: {}", id);

    let ledger = task
        .ledger
        .ok_or_else(|| invalid_task_format("Missing ledger"))?;
    let code: u16 = task
        .code
        .ok_or_else(|| invalid_task_format("Missing code"))?
        .try_into()
        .map_err(|_| invalid_task_format("code exceeds u16 range"))?;

    info!("Ledger={}, Code={}", ledger, code);

    let mut account = TBAccount::new(id, ledger, code);
    if let Some(v) = &task.user_data_128 {
        account = account.with_user_data_128(proto_to_uint128(v));
    }
    if let Some(v) = task.user_data_64 {
        account = account.with_user_data_64(v);
    }
    if let Some(v) = task.user_data_32 {
        account = account.with_user_data_32(v);
    }

    // ✅ No .lock().await - client is already thread-safe
    let client = get_tb_client().await?;
    let result = client.create_accounts(vec![account]).await;

    match result {
        Ok(()) => {
            info!("Account created successfully for id={}", id);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: true,
                message: "Account created".into(),
                account_result: None,
                transfer_result: None,
            })
        }
        Err(CreateAccountsError::Api(api_error)) => {
            let error_msg = api_error
                .as_slice()
                .iter()
                .map(|individual_error| individual_error.inner().to_string())
                .collect::<Vec<_>>()
                .join(";");
            let err = AppError::new(ErrorKind::TigerBeetle, "create_account")
                .context("account_id", id.to_string())
                .message(format!("Failed to create account: {}", error_msg));
            error!("{}", err);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            })
        }
        Err(e) => {
            let err = AppError::new(ErrorKind::TigerBeetle, "create_account")
                .context("account_id", id.to_string())
                .message(format!("Failed to create account: {}", e));
            error!("{}", err);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            })
        }
    }
}

async fn handle_create_account_batch(task: &TaskRequest) -> Result<TaskResponse> {
    info!("handle_create_account_batch called for task_id={}", task.id);
    if task.account_batch.is_empty() {
        let err = AppError::new(ErrorKind::InvalidInput, "create_account_batch")
            .message("account_batch cannot be empty");
        error!("{}", err);
        return Ok(TaskResponse {
            id: task.id.clone(),
            task_type: task.task_type.to_string(),
            success: false,
            message: err.get_message().to_string(),
            account_result: None,
            transfer_result: None,
        });
    }

    let mut accounts = Vec::with_capacity(task.account_batch.len());
    for (i, req) in task.account_batch.iter().enumerate() {
        let id =
            proto_to_uint128(req.id.as_ref().ok_or_else(|| {
                invalid_task_format(format!("Missing id for account_batch[{i}]"))
            })?);
        info!("Processing batch account {} with id={}", i, id);

        let ledger = req.ledger;
        let code: u16 = req.code.try_into().map_err(|_| {
            invalid_task_format(format!("code exceeds u16 range for account_batch[{i}]"))
        })?;

        let mut acc = TBAccount::new(id, ledger, code);
        if let Some(v) = &req.user_data_128 {
            acc = acc.with_user_data_128(proto_to_uint128(v));
        }
        if let Some(v) = req.user_data_64 {
            acc = acc.with_user_data_64(v);
        }
        if let Some(v) = req.user_data_32 {
            acc = acc.with_user_data_32(v);
        }
        accounts.push(acc);
    }

    // ✅ No .lock().await
    let client = get_tb_client().await?;
    let result = client.create_accounts(accounts).await;

    match result {
        Ok(()) => {
            info!("Batch accounts created successfully");
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: true,
                message: format!("Created {} accounts", task.account_batch.len()),
                account_result: None,
                transfer_result: None,
            })
        }
        Err(CreateAccountsError::Api(api_error)) => {
            let err_details = api_error
                .as_slice()
                .iter()
                .map(|individual_error| {
                    format!(
                        "Account[{}]: {}",
                        individual_error.index(),
                        individual_error.inner()
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            let err = AppError::new(ErrorKind::TigerBeetle, "create_account_batch")
                .message(format!("Failed to create account batch: {}", err_details))
                .context("batch_size", task.account_batch.len().to_string());
            error!("{}", err);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            })
        }
        Err(CreateAccountsError::Send(send_error)) => {
            let err = AppError::new(ErrorKind::NetworkTimeout, "create_account_batch")
                .message(format!("Network/Transport Error: {}", send_error))
                .context("batch_size", task.account_batch.len().to_string());
            error!("{}", err);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            })
        }
        Err(e) => {
            let err = AppError::new(ErrorKind::TigerBeetle, "create_account_batch")
                .message(format!("Failed to create account batch: {}", e))
                .context("batch_size", task.account_batch.len().to_string());
            error!("{}", err);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            })
        }
    }
}

async fn handle_lookup_accounts(task: &TaskRequest) -> Result<TaskResponse> {
    info!("handle_lookup_accounts called for task_id={}", task.id);
    let ids: Vec<u128> = task.lookup_ids.iter().map(proto_to_uint128).collect();
    info!("Looking up {} accounts: {:?}", ids.len(), ids);

    // ✅ No .lock().await
    let client = get_tb_client().await?;
    let accounts = client.lookup_accounts(ids.clone()).await.map_err(|e| {
        AppError::new(ErrorKind::TigerBeetle, "lookup_accounts")
            .message(format!("Failed to lookup accounts: {}", e))
            .context("account_count", ids.len().to_string())
            .into_anyhow()
    })?;

    info!("Lookup completed, {} accounts found", accounts.len());

    let proto_accounts: Vec<Account> = accounts
        .into_iter()
        .map(|a| Account {
            id: Some(uint128_to_proto(a.id())),
            debits_pending: Some(uint128_to_proto(a.debits_pending())),
            debits_posted: Some(uint128_to_proto(a.debits_posted())),
            credits_pending: Some(uint128_to_proto(a.credits_pending())),
            credits_posted: Some(uint128_to_proto(a.credits_posted())),
            ledger: a.ledger(),
            code: a.code() as u32,
            flags: a.flags().bits() as u32,
            timestamp: system_time_to_u64(a.timestamp()),
        })
        .collect();

    let count = i32::try_from(proto_accounts.len()).unwrap_or(i32::MAX);
    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.to_string(),
        success: true,
        message: "Accounts found".into(),
        account_result: Some(AccountResult {
            count,
            accounts: proto_accounts,
        }),
        transfer_result: None,
    })
}

async fn handle_create_transfer(task: &TaskRequest) -> Result<TaskResponse> {
    info!("handle_create_transfer called for task_id={}", task.id);
    let debit = proto_to_uint128(
        task.debit_account_id
            .as_ref()
            .ok_or_else(|| invalid_task_format("Missing debit_account_id"))?,
    );
    let credit = proto_to_uint128(
        task.credit_account_id
            .as_ref()
            .ok_or_else(|| invalid_task_format("Missing credit_account_id"))?,
    );
    let amount = proto_to_uint128(
        task.amount
            .as_ref()
            .ok_or_else(|| invalid_task_format("Missing amount"))?,
    );

    // Validate amount is non-zero
    if amount == 0 {
        let err = AppError::new(ErrorKind::InvalidInput, "create_transfer")
            .message("Transfer amount must be greater than 0")
            .context("debit_account", debit.to_string())
            .context("credit_account", credit.to_string());
        error!("{}", err);
        return Ok(TaskResponse {
            id: task.id.clone(),
            task_type: task.task_type.to_string(),
            success: false,
            message: err.get_message().to_string(),
            account_result: None,
            transfer_result: None,
        });
    }

    // Validate accounts are different
    if debit == credit {
        let err = AppError::new(ErrorKind::InvalidInput, "create_transfer")
            .message("Debit and credit accounts must be different")
            .context("account_id", debit.to_string());
        error!("{}", err);
        return Ok(TaskResponse {
            id: task.id.clone(),
            task_type: task.task_type.to_string(),
            success: false,
            message: err.get_message().to_string(),
            account_result: None,
            transfer_result: None,
        });
    }

    let ledger = task
        .ledger
        .ok_or_else(|| invalid_task_format("Missing ledger"))?;
    let code: u16 = task
        .code
        .ok_or_else(|| invalid_task_format("Missing code"))?
        .try_into()
        .map_err(|_| invalid_task_format("code exceeds u16 range"))?;

    // Use provided transfer_id or generate new one
    let transfer_id = task
        .transfer_id
        .as_ref()
        .map(proto_to_uint128)
        .unwrap_or_else(|| Uuid::new_v4().as_u128());

    info!(
        "Creating transfer id={} debit={} credit={} amount={}",
        transfer_id, debit, credit, amount
    );

    let transfer = TBTransfer::new(transfer_id)
        .with_debit_account_id(debit)
        .with_credit_account_id(credit)
        .with_amount(amount)
        .with_ledger(ledger)
        .with_code(code);

    // ✅ No .lock().await
    let client = get_tb_client().await?;
    let result = client.create_transfers(vec![transfer]).await;

    match result {
        Ok(()) => {
            info!("Transfer created successfully with id={}", transfer_id);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: true,
                message: "Transfer created".into(),
                account_result: None,
                transfer_result: None,
            })
        }
        Err(CreateTransfersError::Api(api_error)) => {
            let details = api_error
                .as_slice()
                .iter()
                .map(|e| format!("transfer[{}]: {}", e.index(), e.inner()))
                .collect::<Vec<_>>()
                .join("; ");
            let err = AppError::new(ErrorKind::TigerBeetle, "create_transfer")
                .message(format!("Failed to create transfer: {}", details))
                .context("transfer_id", transfer_id.to_string())
                .context("debit_account", debit.to_string())
                .context("credit_account", credit.to_string())
                .context("amount", amount.to_string());
            error!("{}", err);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            })
        }
        Err(CreateTransfersError::Send(send_error)) => {
            let err = AppError::new(ErrorKind::NetworkTimeout, "create_transfer")
                .message(format!("Network/Transport Error: {}", send_error))
                .context("transfer_id", transfer_id.to_string());
            error!("{}", err);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            })
        }
        Err(e) => {
            let err = AppError::new(ErrorKind::TigerBeetle, "create_transfer")
                .message(format!("Unexpected Error: {}", e))
                .context("transfer_id", transfer_id.to_string());
            error!("{}", err);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            })
        }
    }
}

async fn handle_create_transfer_batch(task: &TaskRequest) -> Result<TaskResponse> {
    info!(
        "handle_create_transfer_batch called for task_id={}",
        task.id
    );
    if task.transfer_batch.is_empty() {
        return Err(
            AppError::new(ErrorKind::InvalidInput, "create_transfer_batch")
                .message("transfer_batch cannot be empty")
                .into_anyhow(),
        );
    }

    let mut transfers = Vec::with_capacity(task.transfer_batch.len());
    for (i, req) in task.transfer_batch.iter().enumerate() {
        let prefix = format!("transfer_batch[{i}]");
        let debit_account_id =
            proto_to_uint128(req.debit_account_id.as_ref().ok_or_else(|| {
                invalid_task_format(format!("{prefix}.debit_account_id missing"))
            })?);
        let credit_account_id =
            proto_to_uint128(req.credit_account_id.as_ref().ok_or_else(|| {
                invalid_task_format(format!("{prefix}.credit_account_id missing"))
            })?);
        let amount = proto_to_uint128(
            req.amount
                .as_ref()
                .ok_or_else(|| invalid_task_format(format!("{prefix}.amount missing")))?,
        );

        // Validate amount is non-zero
        if amount == 0 {
            let err = AppError::new(ErrorKind::InvalidInput, "create_transfer_batch")
                .message(format!("{prefix}: Transfer amount must be greater than 0"))
                .context("index", i.to_string());
            error!("{}", err);
            return Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            });
        }

        // Validate accounts are different
        if debit_account_id == credit_account_id {
            let err = AppError::new(ErrorKind::InvalidInput, "create_transfer_batch")
                .message(format!(
                    "{prefix}: Debit and credit accounts must be different"
                ))
                .context("index", i.to_string())
                .context("account_id", debit_account_id.to_string());
            error!("{}", err);
            return Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            });
        }

        let transfer_id = req
            .id
            .as_ref()
            .map(proto_to_uint128)
            .unwrap_or_else(|| Uuid::new_v4().as_u128());
        info!("Processing transfer batch {} with id={}", i, transfer_id);

        let ledger = req.ledger.unwrap_or(0);
        let code: u16 = req
            .code
            .unwrap_or(0)
            .try_into()
            .map_err(|_| invalid_task_format(format!("{prefix}: code exceeds u16 range")))?;

        let t = TBTransfer::new(transfer_id)
            .with_debit_account_id(debit_account_id)
            .with_credit_account_id(credit_account_id)
            .with_amount(amount)
            .with_ledger(ledger)
            .with_code(code);
        transfers.push(t);
    }

    // ✅ No .lock().await
    let client = get_tb_client().await?;
    let result = client.create_transfers(transfers).await;

    match result {
        Ok(()) => {
            info!("Batch transfers created successfully");
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: true,
                message: format!("Created {} transfers", task.transfer_batch.len()),
                account_result: None,
                transfer_result: None,
            })
        }
        Err(CreateTransfersError::Api(api_error)) => {
            let details = api_error
                .as_slice()
                .iter()
                .map(|individual_error| {
                    format!(
                        "Transfer[{}]: {}",
                        individual_error.index(),
                        individual_error.inner()
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            let err = AppError::new(ErrorKind::TigerBeetle, "create_transfer_batch")
                .message(format!("Failed to create transfer batch: {}", details))
                .context("batch_size", task.transfer_batch.len().to_string());
            error!("{}", err);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            })
        }
        Err(CreateTransfersError::Send(send_error)) => {
            let err = AppError::new(ErrorKind::NetworkTimeout, "create_transfer_batch")
                .message(format!("Network/Transport Error: {}", send_error))
                .context("batch_size", task.transfer_batch.len().to_string());
            error!("{}", err);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            })
        }
        Err(e) => {
            let err = AppError::new(ErrorKind::TigerBeetle, "create_transfer_batch")
                .message(format!("Unexpected error creating transfer batch: {}", e))
                .context("batch_size", task.transfer_batch.len().to_string());
            error!("{}", err);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            })
        }
    }
}
async fn handle_lookup_transfers(task: &TaskRequest) -> Result<TaskResponse> {
    info!("handle_lookup_transfers called for task_id={}", task.id);

    if task.lookup_ids.is_empty() {
        return Err(AppError::new(ErrorKind::InvalidInput, "lookup_transfers")
            .message("lookup_ids cannot be empty")
            .into_anyhow());
    }

    let ids: Vec<u128> = task.lookup_ids.iter().map(proto_to_uint128).collect();
    info!("Looking up {} transfers", ids.len());

    let client = get_tb_client().await?;
    let result = client.lookup_transfers(ids.clone()).await;

    match result {
        Ok(transfers) => {
            info!("Lookup completed, {} transfers found", transfers.len());
            let proto_transfers: Vec<Transfer> = transfers
                .into_iter()
                .map(|t| Transfer {
                    id: Some(uint128_to_proto(t.id())),
                    debit_account_id: Some(uint128_to_proto(t.debit_account_id())),
                    credit_account_id: Some(uint128_to_proto(t.credit_account_id())),
                    amount: Some(uint128_to_proto(t.amount())),
                    ledger: t.ledger(),
                    code: t.code() as u32,
                    flags: t.flags().bits() as u32,
                    timestamp: system_time_to_u64(t.timestamp()),
                })
                .collect();

            let count = proto_transfers.len() as i32;
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: true,
                message: format!("Found {count} transfers"),
                account_result: None,
                transfer_result: Some(TransferResult {
                    count,
                    transfers: proto_transfers,
                }),
            })
        }
        Err(e) => {
            let err = AppError::new(ErrorKind::TigerBeetle, "lookup_transfers")
                .message(format!("Failed to lookup transfers: {}", e))
                .context("transfer_count", ids.len().to_string());

            error!("{}", err);
            Ok(TaskResponse {
                id: task.id.clone(),
                task_type: task.task_type.to_string(),
                success: false,
                message: err.get_message().to_string(),
                account_result: None,
                transfer_result: None,
            })
        }
    }
}

pub fn encode_response(resp: &TaskResponse, ct: ContentType) -> Result<Vec<u8>> {
    info!("Encoding TaskResponse as {}", ct.as_str());

    let result = match ct {
        ContentType::Json => {
            let s: TaskResponse = resp.clone();
            json_serialize(&s).map_err(|e| {
                AppError::new(ErrorKind::InvalidOperation, "encode_response")
                    .message(format!("Failed to serialize to JSON: {}", e))
                    .context("content_type", "application/json")
                    .into_anyhow()
            })
        }
        ContentType::Protobuf => {
            let mut buf = Vec::with_capacity(resp.encoded_len());
            resp.encode(&mut buf).map_err(|e| {
                AppError::new(ErrorKind::ProtobufDecode, "encode_response")
                    .message(format!("Failed to encode protobuf: {}", e))
                    .context("content_type", "application/x-protobuf")
                    .into_anyhow()
            })?;
            Ok(buf)
        }
    };

    result
}
