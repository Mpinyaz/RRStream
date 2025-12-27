use crate::responseconsumer::get_tb_client;
use crate::task::{
    Account, AccountResult, TaskRequest, TaskResponse, TaskType, Transfer, TransferResult, UInt128,
};
use anyhow::Result;
use prost::Message as ProstMessage;
use rabbitmq_stream_client::types::Message as StreamMessage;
use serde_json::to_vec as json_serialize;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tigerbeetle_unofficial::{Account as TBAccount, Transfer as TBTransfer};
use tracing::info;
use uuid::Uuid;

/* ----------------------------- Utilities ----------------------------- */

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

/* ----------------------------- ContentType ----------------------------- */

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

/* ----------------------------- Decode ----------------------------- */

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
    TaskRequest::decode(bytes).map_err(|e| anyhow::anyhow!(e))
}

fn decode_json(bytes: &[u8]) -> Result<TaskRequest> {
    // Attempt deserialization
    match serde_json::from_slice::<TaskRequest>(bytes) {
        Ok(task) => {
            info!("✅ JSON decoded successfully: task_id={}", task.id);
            Ok(task)
        }
        Err(e) => Err(e.into()),
    }
}

/* ----------------------------- Entry Point ----------------------------- */

pub async fn process_task(msg: &StreamMessage) -> Result<TaskResponse> {
    info!("Starting process_task");

    let decoded = decode_message(msg)?;

    info!(
        "Processing task id={} type={:?}",
        decoded.task.id, decoded.task.task_type
    );

    let task_type = TaskType::try_from(decoded.task.task_type)
        .map_err(|_| anyhow::anyhow!("Invalid task_type value"))?;

    let response = match task_type {
        TaskType::CreateAccount => handle_create_account(&decoded.task).await,
        TaskType::BatchAccounts => handle_create_account_batch(&decoded.task).await,
        TaskType::LookupAccounts => handle_lookup_accounts(&decoded.task).await,

        TaskType::CreateTransfer => handle_create_transfer(&decoded.task).await,
        TaskType::BatchTransfers => handle_create_transfer_batch(&decoded.task).await,
        TaskType::LookupTransfers => handle_lookup_transfers(&decoded.task).await,

        TaskType::Unknown => Err(anyhow::anyhow!("task_type UNKNOWN is invalid")),
    };

    info!("Finished process_task for task_id={}", decoded.task.id);
    response
}

/* ----------------------------- Accounts ----------------------------- */

async fn handle_create_account(task: &TaskRequest) -> Result<TaskResponse> {
    info!("handle_create_account called for task_id={}", task.id);

    let id = proto_to_uint128(
        task.account_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("account_id required"))?,
    );
    info!("Account id decoded: {}", id);

    let ledger = task
        .ledger
        .ok_or_else(|| anyhow::anyhow!("ledger required"))?;
    let code: u16 = task
        .code
        .ok_or_else(|| anyhow::anyhow!("code required"))?
        .try_into()?;
    info!("Ledger={}, Code={}", ledger, code);

    let mut account = TBAccount::new(id, ledger, code);

    if let Some(v) = &task.user_data_128 {
        account = account.with_user_data_128(proto_to_uint128(v));
        info!("Applied user_data_128={}", proto_to_uint128(v));
    }
    if let Some(v) = task.user_data_64 {
        account = account.with_user_data_64(v);
        info!("Applied user_data_64={}", v);
    }
    if let Some(v) = task.user_data_32 {
        account = account.with_user_data_32(v);
        info!("Applied user_data_32={}", v);
    }

    get_tb_client()
        .await?
        .lock()
        .await
        .create_accounts(vec![account])
        .await?;
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

async fn handle_create_account_batch(task: &TaskRequest) -> Result<TaskResponse> {
    info!("handle_create_account_batch called for task_id={}", task.id);
    let tb = get_tb_client().await?;
    let mut accounts = Vec::new();

    for (i, req) in task.account_batch.iter().enumerate() {
        let id = proto_to_uint128(req.id.as_ref().unwrap());
        info!("Processing batch account {} with id={}", i, id);
        let mut acc = TBAccount::new(id, req.ledger, req.code as u16);

        if let Some(v) = &req.user_data_128 {
            acc = acc.with_user_data_128(proto_to_uint128(v));
            info!("Applied user_data_128={}", proto_to_uint128(v));
        }

        accounts.push(acc);
    }

    tb.lock().await.create_accounts(accounts).await?;
    info!("Batch accounts created successfully");

    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.to_string(),
        success: true,
        message: "Account batch created".into(),
        account_result: None,
        transfer_result: None,
    })
}

async fn handle_lookup_accounts(task: &TaskRequest) -> Result<TaskResponse> {
    info!("handle_lookup_accounts called for task_id={}", task.id);
    let ids: Vec<u128> = task.lookup_ids.iter().map(proto_to_uint128).collect();
    info!("Looking up {} accounts: {:?}", ids.len(), ids);

    let accounts = get_tb_client()
        .await?
        .lock()
        .await
        .lookup_accounts(ids)
        .await?;
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

/* ----------------------------- Transfers ----------------------------- */

async fn handle_create_transfer(task: &TaskRequest) -> Result<TaskResponse> {
    info!("handle_create_transfer called for task_id={}", task.id);
    let transfer_id = Uuid::new_v4().as_u128();
    info!("Generated transfer_id={}", transfer_id);

    let debit = proto_to_uint128(task.debit_account_id.as_ref().unwrap());
    let credit = proto_to_uint128(task.credit_account_id.as_ref().unwrap());
    let amount = proto_to_uint128(task.amount.as_ref().unwrap());
    info!(
        "Creating transfer debit={} credit={} amount={}",
        debit, credit, amount
    );

    let ledger = task.ledger.unwrap();
    let code: u16 = task.code.unwrap().try_into()?;

    let transfer = TBTransfer::new(transfer_id)
        .with_debit_account_id(debit)
        .with_credit_account_id(credit)
        .with_amount(amount)
        .with_ledger(ledger)
        .with_code(code);

    get_tb_client()
        .await?
        .lock()
        .await
        .create_transfers(vec![transfer])
        .await?;
    info!("Transfer created successfully with id={}", transfer_id);

    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.clone().to_string(),
        success: true,
        message: "Transfer created".into(),
        account_result: None,
        transfer_result: None,
    })
}

async fn handle_create_transfer_batch(task: &TaskRequest) -> Result<TaskResponse> {
    info!(
        "handle_create_transfer_batch called for task_id={}",
        task.id
    );
    let tb = get_tb_client().await?;
    let mut transfers = Vec::new();

    for (i, req) in task.transfer_batch.iter().enumerate() {
        let id = Uuid::new_v4().as_u128();
        info!("Processing transfer batch {} with generated id={}", i, id);

        let t = TBTransfer::new(id)
            .with_debit_account_id(proto_to_uint128(req.debit_account_id.as_ref().unwrap()))
            .with_credit_account_id(proto_to_uint128(req.credit_account_id.as_ref().unwrap()))
            .with_amount(proto_to_uint128(req.amount.as_ref().unwrap()))
            .with_ledger(req.ledger.unwrap_or(0))
            .with_code(req.code.unwrap_or(0) as u16);

        transfers.push(t);
    }

    tb.lock().await.create_transfers(transfers).await?;
    info!("Batch transfers created successfully");

    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.to_string(),
        success: true,
        message: "Transfer batch created".into(),
        account_result: None,
        transfer_result: None,
    })
}

async fn handle_lookup_transfers(task: &TaskRequest) -> Result<TaskResponse> {
    info!("handle_lookup_transfers called for task_id={}", task.id);
    let ids: Vec<u128> = task.lookup_ids.iter().map(proto_to_uint128).collect();
    info!("Looking up {} transfers: {:?}", ids.len(), ids);

    let transfers = get_tb_client()
        .await?
        .lock()
        .await
        .lookup_transfers(ids)
        .await?;
    info!("Lookup completed, {} transfers found", transfers.len());

    let proto: Vec<Transfer> = transfers
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

    let count = i32::try_from(proto.len()).unwrap_or(i32::MAX);

    Ok(TaskResponse {
        id: task.id.clone(),
        task_type: task.task_type.to_string(),
        success: true,
        message: "Transfers found".into(),
        account_result: None,
        transfer_result: Some(TransferResult {
            count,
            transfers: proto,
        }),
    })
}

/* ----------------------------- Encode Response ----------------------------- */

pub fn encode_response(resp: &TaskResponse, ct: ContentType) -> Result<Vec<u8>> {
    info!("Encoding TaskResponse as {}", ct.as_str());
    Ok(match ct {
        ContentType::Json => {
            let s: TaskResponse = resp.clone();
            json_serialize(&s)?
        }
        ContentType::Protobuf => {
            let mut buf = Vec::with_capacity(resp.encoded_len());
            resp.encode(&mut buf)?;
            buf
        }
    })
}
