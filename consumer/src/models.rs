use crate::task::*;
use serde::{Deserialize, Serialize};

// Manually define serializable versions of the oneof types

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum SerializablePayloadOperation {
    #[serde(rename = "create_account")]
    CreateAccount(CreateAccountRequest),
    #[serde(rename = "create_account_batch")]
    CreateAccountBatch(CreateAccountBatchRequest),
    #[serde(rename = "lookup_account")]
    LookupAccount(LookupAccountRequest),
    #[serde(rename = "query_account")]
    QueryAccount(QueryAccountRequest),
    #[serde(rename = "create_transfer")]
    CreateTransfer(CreateTransferRequest),
    #[serde(rename = "create_transfer_batch")]
    CreateTransferBatch(CreateTransferBatchRequest),
    #[serde(rename = "lookup_transfer")]
    LookupTransfer(LookupTransferRequest),
    #[serde(rename = "query_transfer")]
    QueryTransfer(QueryTransferRequest),
    #[serde(rename = "post_pending_transfer")]
    PostPendingTransfer(PostPendingTransferRequest),
    #[serde(rename = "void_pending_transfer")]
    VoidPendingTransfer(VoidPendingTransferRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializablePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<SerializablePayloadOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum SerializableQueryResultData {
    #[serde(rename = "accounts")]
    Accounts(AccountResult),
    #[serde(rename = "transfers")]
    Transfers(TransferResult),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableQueryResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<SerializableQueryResultData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableTaskRequest {
    pub id: String,
    pub task_type: String,
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<SerializablePayload>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableTaskResponse {
    pub id: String,
    pub task_type: String,
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_result: Option<AccountResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_result: Option<TransferResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_result: Option<SerializableQueryResult>,
}

// Conversion implementations

impl From<Payload> for SerializablePayload {
    fn from(payload: Payload) -> Self {
        SerializablePayload {
            operation: payload.operation.map(|op| match op {
                payload::Operation::CreateAccount(v) => {
                    SerializablePayloadOperation::CreateAccount(v)
                }
                payload::Operation::CreateAccountBatch(v) => {
                    SerializablePayloadOperation::CreateAccountBatch(v)
                }
                payload::Operation::LookupAccount(v) => {
                    SerializablePayloadOperation::LookupAccount(v)
                }
                payload::Operation::QueryAccount(v) => {
                    SerializablePayloadOperation::QueryAccount(v)
                }
                payload::Operation::CreateTransfer(v) => {
                    SerializablePayloadOperation::CreateTransfer(v)
                }
                payload::Operation::CreateTransferBatch(v) => {
                    SerializablePayloadOperation::CreateTransferBatch(v)
                }
                payload::Operation::LookupTransfer(v) => {
                    SerializablePayloadOperation::LookupTransfer(v)
                }
                payload::Operation::QueryTransfer(v) => {
                    SerializablePayloadOperation::QueryTransfer(v)
                }
                payload::Operation::PostPendingTransfer(v) => {
                    SerializablePayloadOperation::PostPendingTransfer(v)
                }
                payload::Operation::VoidPendingTransfer(v) => {
                    SerializablePayloadOperation::VoidPendingTransfer(v)
                }
            }),
        }
    }
}

impl From<SerializablePayload> for Payload {
    fn from(payload: SerializablePayload) -> Self {
        Payload {
            operation: payload.operation.map(|op| match op {
                SerializablePayloadOperation::CreateAccount(v) => {
                    payload::Operation::CreateAccount(v)
                }
                SerializablePayloadOperation::CreateAccountBatch(v) => {
                    payload::Operation::CreateAccountBatch(v)
                }
                SerializablePayloadOperation::LookupAccount(v) => {
                    payload::Operation::LookupAccount(v)
                }
                SerializablePayloadOperation::QueryAccount(v) => {
                    payload::Operation::QueryAccount(v)
                }
                SerializablePayloadOperation::CreateTransfer(v) => {
                    payload::Operation::CreateTransfer(v)
                }
                SerializablePayloadOperation::CreateTransferBatch(v) => {
                    payload::Operation::CreateTransferBatch(v)
                }
                SerializablePayloadOperation::LookupTransfer(v) => {
                    payload::Operation::LookupTransfer(v)
                }
                SerializablePayloadOperation::QueryTransfer(v) => {
                    payload::Operation::QueryTransfer(v)
                }
                SerializablePayloadOperation::PostPendingTransfer(v) => {
                    payload::Operation::PostPendingTransfer(v)
                }
                SerializablePayloadOperation::VoidPendingTransfer(v) => {
                    payload::Operation::VoidPendingTransfer(v)
                }
            }),
        }
    }
}

impl From<TaskRequest> for SerializableTaskRequest {
    fn from(req: TaskRequest) -> Self {
        SerializableTaskRequest {
            id: req.id,
            task_type: req.task_type,
            content_type: req.content_type,
            payload: req.payload.map(SerializablePayload::from),
            created_at: req.created_at,
            priority: req.priority,
            retry_count: req.retry_count,
        }
    }
}

impl From<SerializableTaskRequest> for TaskRequest {
    fn from(req: SerializableTaskRequest) -> Self {
        TaskRequest {
            id: req.id,
            task_type: req.task_type,
            content_type: req.content_type,
            payload: req.payload.map(Payload::from),
            created_at: req.created_at,
            priority: req.priority,
            retry_count: req.retry_count,
        }
    }
}

impl From<QueryResult> for SerializableQueryResult {
    fn from(result: QueryResult) -> Self {
        SerializableQueryResult {
            result: result.result.map(|r| match r {
                query_result::Result::Accounts(v) => SerializableQueryResultData::Accounts(v),
                query_result::Result::Transfers(v) => SerializableQueryResultData::Transfers(v),
            }),
        }
    }
}

impl From<SerializableQueryResult> for QueryResult {
    fn from(result: SerializableQueryResult) -> Self {
        QueryResult {
            result: result.result.map(|r| match r {
                SerializableQueryResultData::Accounts(v) => query_result::Result::Accounts(v),
                SerializableQueryResultData::Transfers(v) => query_result::Result::Transfers(v),
            }),
        }
    }
}

impl From<TaskResponse> for SerializableTaskResponse {
    fn from(resp: TaskResponse) -> Self {
        SerializableTaskResponse {
            id: resp.id,
            task_type: resp.task_type,
            success: resp.success,
            message: resp.message,
            account_result: resp.account_result,
            transfer_result: resp.transfer_result,
            query_result: resp.query_result.map(SerializableQueryResult::from),
        }
    }
}

impl From<SerializableTaskResponse> for TaskResponse {
    fn from(resp: SerializableTaskResponse) -> Self {
        TaskResponse {
            id: resp.id,
            task_type: resp.task_type,
            success: resp.success,
            message: resp.message,
            account_result: resp.account_result,
            transfer_result: resp.transfer_result,
            query_result: resp.query_result.map(QueryResult::from),
        }
    }
}
