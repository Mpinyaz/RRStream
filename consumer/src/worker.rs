use crate::models::{SerializableTaskRequest, SerializableTaskResponse};
use crate::{
    responseconsumer::get_tb_client,
    task::{CreateTransferBatchRequest, CreateTransferRequest, TaskRequest, TaskResponse, UInt128},
};
use anyhow::Result;
use prost::Message as ProstMessage;
use rabbitmq_stream_client::types::Message as StreamMessage;
use serde_json::to_vec as json_serialize;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tigerbeetle_unofficial::account::Flags as AccFlags;
use tigerbeetle_unofficial::transfer::Flags;
use tigerbeetle_unofficial::{Account, Transfer};
use tracing::{debug, field::debug, info, warn};
use uuid::Uuid;

fn decode_json(bytes: &[u8]) -> Result<TaskRequest> {
    let serializable: SerializableTaskRequest = serde_json::from_slice(bytes)?;
    Ok(serializable.into())
}

pub fn encode_response_json(resp: &TaskResponse) -> Result<Vec<u8>> {
    let serializable: SerializableTaskResponse = resp.clone().into();
    Ok(serde_json::to_vec(&serializable)?)
}

pub mod transfer_codes {
    pub const PAYMENT: u16 = 1;
    pub const REFUND: u16 = 2;
    pub const FEE: u16 = 3;
    pub const PAYOUT: u16 = 4;
}

pub fn uint128_to_proto(value: u128) -> UInt128 {
    UInt128 {
        low: (value & 0xFFFFFFFFFFFFFFFF) as u64,
        high: (value >> 64) as u64,
    }
}
fn system_time_to_u64(ts: SystemTime) -> u64 {
    ts.duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64
}
/// Convert protobuf UInt128 to u128
pub fn proto_to_uint128(proto: &UInt128) -> u128 {
    ((proto.high as u128) << 64) | (proto.low as u128)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    Json,
    Protobuf,
}

impl FromStr for ContentType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "application/json" => Ok(Self::Json),
            "application/x-protobuf" => Ok(Self::Protobuf),
            _ => Ok(Self::Protobuf), // Default to protobuf
        }
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
    pub correlation_id: Option<String>,
}

impl DecodedMessage {
    pub fn new(
        task: TaskRequest,
        content_type: ContentType,
        correlation_id: Option<String>,
    ) -> Self {
        Self {
            task,
            content_type,
            correlation_id,
        }
    }
}

pub fn decode_message(msg: &StreamMessage) -> Result<DecodedMessage> {
    let bytes = msg
        .data()
        .ok_or_else(|| anyhow::anyhow!("Message contains no data"))?;
    let properties = msg.properties();
    let content_type_string = properties
        .and_then(|p| p.content_type.as_ref())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "application/x-protobuf".to_string());
    let correlation_id =
        properties.and_then(|p| p.correlation_id.as_ref().map(|cid| format!("{cid:?}")));
    let content_type = ContentType::from_str(&content_type_string).unwrap_or(ContentType::Protobuf);

    debug!(
        "Decoding message: content_type={}, size={} bytes, correlation_id={:?}",
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
    TaskRequest::decode(bytes).map_err(|e| anyhow::anyhow!(e))
}

/// Processes a decoded message and returns a TaskResponse
pub async fn process_task(msg: &StreamMessage) -> Result<TaskResponse> {
    let decoded = decode_message(msg)?;
    info!(
        "Processing task: id={}, type={}, format={}, correlation_id={:?}",
        decoded.task.id,
        decoded.task.task_type,
        decoded.content_type.as_str(),
        decoded.correlation_id
    );
    debug!("Task payload: {:?}", decoded.task.payload);

    // Check if payload exists
    let payload = decoded
        .task
        .payload
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Missing payload"))?;

    // Route based on payload operation
    match &payload.operation {
        Some(crate::task::payload::Operation::CreateAccount(account_req)) => {
            handle_create_account(&decoded.task, account_req).await
        }
        Some(crate::task::payload::Operation::CreateAccountBatch(batch_req)) => {
            handle_create_account_batch(&decoded.task, batch_req).await
        }
        Some(crate::task::payload::Operation::LookupAccount(lookup_req)) => {
            handle_lookup_account(&decoded.task, lookup_req).await
        }
        Some(crate::task::payload::Operation::QueryAccount(query_req)) => {
            debug(query_req);
            todo!()
        }
        Some(crate::task::payload::Operation::CreateTransfer(transfer_req)) => {
            handle_create_transfer(&decoded.task, transfer_req).await
        }
        Some(crate::task::payload::Operation::CreateTransferBatch(batch_req)) => {
            handle_create_transfer_batch(&decoded.task, batch_req).await
        }
        Some(crate::task::payload::Operation::LookupTransfer(lookup_req)) => {
            handle_lookup_transfer(&decoded.task, lookup_req).await
        }
        Some(crate::task::payload::Operation::QueryTransfer(query_req)) => {
            handle_query_transfer(&decoded.task, query_req).await
        }
        Some(crate::task::payload::Operation::PostPendingTransfer(post_req)) => {
            handle_post_pending_transfer(&decoded.task, post_req).await
        }
        Some(crate::task::payload::Operation::VoidPendingTransfer(void_req)) => {
            handle_void_pending_transfer(&decoded.task, void_req).await
        }
        None => {
            // Fallback to old task type routing
            let result = match decoded.task.task_type.as_str() {
                "email_notification" => handle_email_notification(&decoded.task).await,
                "process_order" => handle_process_order(&decoded.task).await,
                "generate_report" => handle_generate_report(&decoded.task).await,
                _ => handle_default(&decoded.task).await,
            };

            match result {
                Ok(message) => Ok(TaskResponse {
                    id: decoded.task.id.clone(),
                    task_type: decoded.task.task_type.clone(),
                    success: true,
                    message,
                    account_result: None,
                    transfer_result: None,
                    query_result: None,
                }),
                Err(e) => {
                    warn!("Task {} processing failed: {}", decoded.task.id, e);
                    Ok(TaskResponse {
                        id: decoded.task.id.clone(),
                        task_type: decoded.task.task_type.clone(),
                        success: false,
                        message: format!("Processing failed: {e}"),
                        account_result: None,
                        transfer_result: None,
                        query_result: None,
                    })
                }
            }
        }
    }
}

async fn handle_email_notification(task: &TaskRequest) -> Result<String> {
    info!("Sending email notification for task {}", task.id);
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    Ok(format!("Email notification sent for task {}", task.id))
}

async fn handle_process_order(task: &TaskRequest) -> Result<String> {
    info!("Processing order for task {}", task.id);
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
    Ok(format!("Order processed for task {}", task.id))
}

async fn handle_generate_report(task: &TaskRequest) -> Result<String> {
    info!("Generating report for task {}", task.id);
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    Ok(format!("Report generated for task {}", task.id))
}

async fn handle_default(task: &TaskRequest) -> Result<String> {
    info!("Handling generic task {}", task.id);
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    Ok(format!("Task {} processed", task.id))
}

// Account handlers
async fn handle_create_account(
    task: &TaskRequest,
    account_req: &crate::task::CreateAccountRequest,
) -> Result<TaskResponse> {
    let account_id = proto_to_uint128(
        account_req
            .id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing account id"))?,
    );

    let code: u16 = account_req
        .code
        .try_into()
        .map_err(|_| anyhow::anyhow!("Code {} too large for u16", account_req.code))?;

    let tb_client = get_tb_client().await?;

    // Build account with all user data BEFORE creating it
    let mut account = Account::new(account_id, account_req.ledger, code);

    // Add optional user data
    if let Some(user_data_128) = &account_req.user_data_128 {
        account = account.with_user_data_128(proto_to_uint128(user_data_128));
    }
    if account_req.user_data_64 != 0 {
        account = account.with_user_data_64(account_req.user_data_64);
    }
    if account_req.user_data_32 != 0 {
        account = account.with_user_data_32(account_req.user_data_32);
    }
    if account_req.flags != 0 {
        let flags_u16: u16 = account_req
            .flags
            .try_into()
            .map_err(|_| anyhow::anyhow!("Flags {} too large for u16", account_req.flags))?;
        if let Some(flags) = AccFlags::from_bits(flags_u16) {
            account = account.with_flags(flags);
        }
    }

    tb_client
        .lock()
        .await
        .create_accounts(vec![account])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create account: {}", e))?;

    info!("Account created: id={}", account_id);

    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.clone(),
        success: true,
        message: format!("Account {account_id} created successfully"),
        account_result: Some(crate::task::AccountResult {
            accounts: vec![crate::task::Account {
                id: account_req.id,
                debits_pending: Some(uint128_to_proto(0)),
                debits_posted: Some(uint128_to_proto(0)),
                credits_pending: Some(uint128_to_proto(0)),
                credits_posted: Some(uint128_to_proto(0)),
                user_data_128: account_req.user_data_128,
                user_data_64: account_req.user_data_64,
                user_data_32: account_req.user_data_32,
                ledger: account_req.ledger,
                code: account_req.code,
                flags: account_req.flags,
                timestamp: 0,
            }],
            count: 1,
        }),
        transfer_result: None,
        query_result: None,
    })
}

async fn handle_create_account_batch(
    task: &TaskRequest,
    _batch_req: &crate::task::CreateAccountBatchRequest,
) -> Result<TaskResponse> {
    // TODO: Implement batch account creation
    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.clone(),
        success: false,
        message: "Batch account creation not yet implemented".to_string(),
        account_result: None,
        transfer_result: None,
        query_result: None,
    })
}

async fn handle_lookup_account(
    task: &TaskRequest,
    lookup_req: &crate::task::LookupAccountRequest,
) -> Result<TaskResponse> {
    let tb_client = get_tb_client().await?;

    let account_ids: Vec<u128> = lookup_req
        .account_ids
        .iter()
        .map(proto_to_uint128)
        .collect();

    let accounts = tb_client
        .lock()
        .await
        .lookup_accounts(account_ids)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to lookup accounts: {}", e))?;

    let proto_accounts: Vec<crate::task::Account> = accounts
        .iter()
        .map(|acc| crate::task::Account {
            id: Some(uint128_to_proto(acc.id())),
            debits_pending: Some(uint128_to_proto(acc.debits_pending())),
            debits_posted: Some(uint128_to_proto(acc.debits_posted())),
            credits_pending: Some(uint128_to_proto(acc.credits_pending())),
            credits_posted: Some(uint128_to_proto(acc.credits_posted())),
            user_data_128: Some(uint128_to_proto(acc.user_data_128())),
            user_data_64: acc.user_data_64(),
            user_data_32: acc.user_data_32(),
            ledger: acc.ledger(),
            code: acc.code() as u32,
            flags: acc.flags().bits() as u32,
            timestamp: system_time_to_u64(acc.timestamp()),
        })
        .collect();

    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.clone(),
        success: true,
        message: format!("Found {} account(s)", proto_accounts.len()),
        account_result: Some(crate::task::AccountResult {
            accounts: proto_accounts.clone(),
            count: proto_accounts.len() as i32,
        }),
        transfer_result: None,
        query_result: None,
    })
}

// Transfer handlers
async fn handle_create_transfer(
    task: &TaskRequest,
    transfer_req: &crate::task::CreateTransferRequest,
) -> Result<TaskResponse> {
    let transfer_id = create_transfer(*transfer_req)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create transfer: {}", e))?;

    let tb_transfer = lookup_transfer(transfer_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Transfer not found after creation"))?;

    let proto_transfer = crate::task::Transfer {
        id: Some(uint128_to_proto(tb_transfer.id())),
        debit_account_id: Some(uint128_to_proto(tb_transfer.debit_account_id())),
        credit_account_id: Some(uint128_to_proto(tb_transfer.credit_account_id())),
        amount: Some(uint128_to_proto(tb_transfer.amount())),
        pending_id: Some(uint128_to_proto(tb_transfer.pending_id())),
        user_data_128: Some(uint128_to_proto(tb_transfer.user_data_128())),
        user_data_64: tb_transfer.user_data_64(),
        user_data_32: tb_transfer.user_data_32(),
        timeout: tb_transfer.timeout() as u64,
        ledger: tb_transfer.ledger(),
        code: tb_transfer.code() as u32,
        flags: tb_transfer.flags().bits() as u32,
        timestamp: system_time_to_u64(tb_transfer.timestamp()),
    };

    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.clone(),
        success: true,
        message: format!("Transfer {transfer_id} created successfully"),
        account_result: None,
        transfer_result: Some(crate::task::TransferResult {
            transfers: vec![proto_transfer],
            count: 1,
        }),
        query_result: None,
    })
}

async fn handle_create_transfer_batch(
    task: &TaskRequest,
    batch_req: &crate::task::CreateTransferBatchRequest,
) -> Result<TaskResponse> {
    let transfer_ids = create_transfers_batch(batch_req.clone())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create transfer batch: {}", e))?;

    info!("Created {} transfers in batch", transfer_ids.len());

    let proto_transfers: Vec<crate::task::Transfer> = transfer_ids
        .iter()
        .zip(&batch_req.transfers)
        .map(|(id, req)| crate::task::Transfer {
            id: Some(uint128_to_proto(*id)),
            debit_account_id: req.debit_account_id,
            credit_account_id: req.credit_account_id,
            amount: req.amount,
            pending_id: req.pending_id,
            user_data_128: req.user_data_128,
            user_data_64: req.user_data_64,
            user_data_32: req.user_data_32,
            timeout: req.timeout,
            ledger: req.ledger,
            code: req.code,
            flags: req.flags,
            timestamp: 0,
        })
        .collect();

    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.clone(),
        success: true,
        message: format!("Created {} transfers successfully", transfer_ids.len()),
        account_result: None,
        transfer_result: Some(crate::task::TransferResult {
            transfers: proto_transfers.clone(),
            count: proto_transfers.len() as i32,
        }),
        query_result: None,
    })
}

async fn handle_lookup_transfer(
    task: &TaskRequest,
    lookup_req: &crate::task::LookupTransferRequest,
) -> Result<TaskResponse> {
    let tb_client = get_tb_client().await?;

    let transfer_ids: Vec<u128> = lookup_req
        .transfer_ids
        .iter()
        .map(proto_to_uint128)
        .collect();

    let transfers = tb_client
        .lock()
        .await
        .lookup_transfers(transfer_ids)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to lookup transfers: {}", e))?;

    let proto_transfers: Vec<crate::task::Transfer> = transfers
        .iter()
        .map(|t| crate::task::Transfer {
            id: Some(uint128_to_proto(t.id())),
            debit_account_id: Some(uint128_to_proto(t.debit_account_id())),
            credit_account_id: Some(uint128_to_proto(t.credit_account_id())),
            amount: Some(uint128_to_proto(t.amount())),
            pending_id: Some(uint128_to_proto(t.pending_id())),
            user_data_128: Some(uint128_to_proto(t.user_data_128())),
            user_data_64: t.user_data_64(),
            user_data_32: t.user_data_32(),
            timeout: t.timeout() as u64,
            ledger: t.ledger(),
            code: t.code() as u32,
            flags: t.flags().bits() as u32,
            timestamp: system_time_to_u64(t.timestamp()),
        })
        .collect();

    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.clone(),
        success: true,
        message: format!("Found {} transfer(s)", proto_transfers.len()),
        account_result: None,
        transfer_result: Some(crate::task::TransferResult {
            transfers: proto_transfers.clone(),
            count: proto_transfers.len() as i32,
        }),
        query_result: None,
    })
}

async fn handle_query_transfer(
    task: &TaskRequest,
    _query_req: &crate::task::QueryTransferRequest,
) -> Result<TaskResponse> {
    // TODO: Implement query transfer
    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.clone(),
        success: false,
        message: "Query transfer not yet implemented".to_string(),
        account_result: None,
        transfer_result: None,
        query_result: None,
    })
}

async fn handle_post_pending_transfer(
    task: &TaskRequest,
    post_req: &crate::task::PostPendingTransferRequest,
) -> Result<TaskResponse> {
    let pending_id = proto_to_uint128(
        post_req
            .pending_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing pending_id"))?,
    );

    let code: u16 = post_req
        .code
        .try_into()
        .map_err(|_| anyhow::anyhow!("Code {} too large for u16", post_req.code))?;

    let _transfer_id = post_pending_transfer(pending_id, post_req.ledger, code).await?;

    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.clone(),
        success: true,
        message: format!("Posted pending transfer {pending_id}"),
        account_result: None,
        transfer_result: Some(crate::task::TransferResult {
            transfers: vec![],
            count: 1,
        }),
        query_result: None,
    })
}

async fn handle_void_pending_transfer(
    task: &TaskRequest,
    void_req: &crate::task::VoidPendingTransferRequest,
) -> Result<TaskResponse> {
    let pending_id = proto_to_uint128(
        void_req
            .pending_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing pending_id"))?,
    );

    let code: u16 = void_req
        .code
        .try_into()
        .map_err(|_| anyhow::anyhow!("Code {} too large for u16", void_req.code))?;

    let _transfer_id = void_pending_transfer(pending_id, void_req.ledger, code).await?;

    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.clone(),
        success: true,
        message: format!("Voided pending transfer {pending_id}"),
        account_result: None,
        transfer_result: Some(crate::task::TransferResult {
            transfers: vec![],
            count: 1,
        }),
        query_result: None,
    })
}

pub async fn create_transfer(transfer: CreateTransferRequest) -> Result<u128> {
    let tb_client = get_tb_client().await?;
    let transfer_id = Uuid::new_v4().as_u128();

    // Extract and convert required fields
    let debit_account_id = proto_to_uint128(
        transfer
            .debit_account_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing debit_account_id"))?,
    );

    let credit_account_id = proto_to_uint128(
        transfer
            .credit_account_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing credit_account_id"))?,
    );

    // Convert amount from UInt128 to u128
    let amount = proto_to_uint128(
        transfer
            .amount
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing amount"))?,
    );

    let code: u16 = transfer
        .code
        .try_into()
        .map_err(|_| anyhow::anyhow!("Code {} too large for u16", transfer.code))?;

    let tb_transfer = Transfer::new(transfer_id)
        .with_debit_account_id(debit_account_id)
        .with_credit_account_id(credit_account_id)
        .with_amount(amount)
        .with_ledger(transfer.ledger)
        .with_code(code);

    tb_client
        .lock()
        .await
        .create_transfers(vec![tb_transfer])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create transfer {}: {}", transfer_id, e))?;

    info!(
        "Transfer created: id={}, debit={}, credit={}, amount={}",
        transfer_id, debit_account_id, credit_account_id, amount
    );

    Ok(transfer_id)
}

pub async fn create_pending_transfer(transfer: CreateTransferRequest) -> Result<u128> {
    let tb_client = get_tb_client().await?;
    let transfer_id = Uuid::new_v4().as_u128();

    // Extract and convert required fields
    let debit_account_id = proto_to_uint128(
        transfer
            .debit_account_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing debit_account_id"))?,
    );

    let credit_account_id = proto_to_uint128(
        transfer
            .credit_account_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing credit_account_id"))?,
    );

    // Convert amount from UInt128 to u128
    let amount = proto_to_uint128(
        transfer
            .amount
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing amount"))?,
    );

    // Convert code from u32 to u16
    let code: u16 = transfer
        .code
        .try_into()
        .map_err(|_| anyhow::anyhow!("Code {} too large for u16", transfer.code))?;

    // Convert timeout from u64 to u32
    let timeout: u32 = transfer
        .timeout
        .try_into()
        .map_err(|_| anyhow::anyhow!("Timeout {} too large for u32", transfer.timeout))?;

    let tb_transfer = Transfer::new(transfer_id)
        .with_debit_account_id(debit_account_id)
        .with_credit_account_id(credit_account_id)
        .with_amount(amount)
        .with_ledger(transfer.ledger)
        .with_code(code)
        .with_timeout(timeout)
        .with_flags(Flags::PENDING);

    tb_client
        .lock()
        .await
        .create_transfers(vec![tb_transfer])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create pending transfer: {}", e))?;

    info!(
        "Pending transfer created: id={}, amount={}, timeout={}s",
        transfer_id, amount, timeout
    );

    Ok(transfer_id)
}

pub async fn post_pending_transfer(pending_id: u128, ledger: u32, code: u16) -> Result<u128> {
    let tb_client = get_tb_client().await?;
    let transfer_id = Uuid::new_v4().as_u128();

    let transfer = Transfer::new(transfer_id)
        .with_pending_id(pending_id)
        .with_ledger(ledger)
        .with_code(code)
        .with_flags(Flags::POST_PENDING_TRANSFER);

    tb_client
        .lock()
        .await
        .create_transfers(vec![transfer])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to post pending transfer: {}", e))?;

    info!("Pending transfer posted: pending_id={}", pending_id);

    Ok(transfer_id)
}

pub async fn void_pending_transfer(pending_id: u128, ledger: u32, code: u16) -> Result<u128> {
    let tb_client = get_tb_client().await?;
    let transfer_id = Uuid::new_v4().as_u128();

    let transfer = Transfer::new(transfer_id)
        .with_pending_id(pending_id)
        .with_ledger(ledger)
        .with_code(code)
        .with_flags(Flags::VOID_PENDING_TRANSFER);

    tb_client
        .lock()
        .await
        .create_transfers(vec![transfer])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to void pending transfer: {}", e))?;

    info!("Pending transfer voided: pending_id={}", pending_id);

    Ok(transfer_id)
}

pub async fn create_transfers_batch(batch: CreateTransferBatchRequest) -> Result<Vec<u128>> {
    let tb_client = get_tb_client().await?;
    let mut transfer_ids = Vec::new();
    let mut tb_transfers = Vec::new();

    for transfer in batch.transfers {
        let transfer_id = Uuid::new_v4().as_u128();
        transfer_ids.push(transfer_id);

        // Extract required fields
        let debit_account_id = proto_to_uint128(
            transfer
                .debit_account_id
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing debit_account_id"))?,
        );

        let credit_account_id = proto_to_uint128(
            transfer
                .credit_account_id
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing credit_account_id"))?,
        );

        // Convert amount from UInt128 to u128
        let amount = proto_to_uint128(
            transfer
                .amount
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing amount"))?,
        );

        // Convert code from u32 to u16
        let code_u16: u16 = transfer
            .code
            .try_into()
            .map_err(|_| anyhow::anyhow!("Code {} too large for u16", transfer.code))?;

        // Build the transfer
        let mut tb_transfer = Transfer::new(transfer_id)
            .with_debit_account_id(debit_account_id)
            .with_credit_account_id(credit_account_id)
            .with_amount(amount)
            .with_ledger(transfer.ledger)
            .with_code(code_u16);

        // Add optional fields if present
        if let Some(pending_id) = transfer.pending_id {
            tb_transfer = tb_transfer.with_pending_id(proto_to_uint128(&pending_id));
        }

        if let Some(user_data_128) = transfer.user_data_128 {
            tb_transfer = tb_transfer.with_user_data_128(proto_to_uint128(&user_data_128));
        }
        if transfer.user_data_64 != 0 {
            tb_transfer = tb_transfer.with_user_data_64(transfer.user_data_64);
        }

        if transfer.user_data_32 != 0 {
            tb_transfer = tb_transfer.with_user_data_32(transfer.user_data_32);
        }

        if transfer.timeout != 0 {
            let timeout_u32: u32 = transfer
                .timeout
                .try_into()
                .map_err(|_| anyhow::anyhow!("Timeout {} too large for u32", transfer.timeout))?;
            tb_transfer = tb_transfer.with_timeout(timeout_u32);
        }

        if transfer.flags != 0 {
            let flags_u16: u16 = transfer
                .flags
                .try_into()
                .map_err(|_| anyhow::anyhow!("Flags {} too large for u16", transfer.flags))?;

            tb_transfer = tb_transfer.with_flags(
                Flags::from_bits(flags_u16)
                    .ok_or_else(|| anyhow::anyhow!("Invalid transfer flags: 0x{:x}", flags_u16))?,
            );
        }

        tb_transfers.push(tb_transfer);
    }

    tb_client
        .lock()
        .await
        .create_transfers(tb_transfers)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create batch transfers: {}", e))?;

    info!("Created {} transfers in batch", transfer_ids.len());

    Ok(transfer_ids)
}

pub async fn lookup_transfer(transfer_id: u128) -> Result<Option<Transfer>> {
    let tb_client = get_tb_client().await?;
    let transfers = tb_client
        .lock()
        .await
        .lookup_transfers(vec![transfer_id])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to lookup transfer: {}", e))?;

    Ok(transfers.into_iter().next())
}

pub async fn get_account_transfers(account_id: u128, limit: u32) -> Result<Vec<Transfer>> {
    let tb_client = get_tb_client().await?;
    // Query all transfers and filter by account
    let filter = tigerbeetle_unofficial::QueryFilter::new(limit);

    let transfers = tb_client
        .lock()
        .await
        .query_transfers(Box::new(filter))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query account transfers: {}", e))?;

    // Filter transfers where account is either debit or credit
    let account_transfers: Vec<_> = transfers
        .into_iter()
        .filter(|t| t.debit_account_id() == account_id || t.credit_account_id() == account_id)
        .collect();

    info!(
        "Found {} transfers for account {}",
        account_transfers.len(),
        account_id
    );

    Ok(account_transfers)
}
pub fn encode_response(resp: &TaskResponse, content_type: ContentType) -> Result<Vec<u8>> {
    match content_type {
        ContentType::Json => {
            // Convert to serializable version for JSON
            let serializable: SerializableTaskResponse = resp.clone().into();
            Ok(json_serialize(&serializable)?)
        }
        ContentType::Protobuf => {
            let mut buf = Vec::with_capacity(resp.encoded_len());
            resp.encode(&mut buf)?;
            Ok(buf)
        }
    }
}
