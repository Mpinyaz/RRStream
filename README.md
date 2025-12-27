# RabbitMQ Stream Task System

A high-performance, distributed task processing system built on **RabbitMQ
Streams**, with a **Golang producer** and a **Rust consumer**, using **Protocol
Buffers** for strict schema and type safety.

---

## Overview

This project implements a **polyglot stream-based task processing architecture**
optimized for throughput, low latency, and correctness.

It is designed around:

- **Go Producer**
  - Validates and publishes tasks
  - Supports JSON → Protobuf conversion
  - Uses `rabbitmq-stream-go-client`

- **Rust Consumer**
  - High-performance task execution
  - Strong typing and memory safety
  - Uses `rabbitmq-stream-client` + `prost`

RabbitMQ Streams are used instead of classic AMQP queues to enable **message
replay, batching, and horizontal scaling**.

---

## Architecture

```
┌─────────────┐
│   Clients   │
└──────┬──────┘
       │
       ▼
┌──────────────┐
│  Go Producer │
│ (CLI / API)  │
└──────┬───────┘
       │  Protobuf / JSON
       ▼
┌──────────────────┐
│ RabbitMQ Streams │
│  (Persistent)    │
└──────┬───────────┘
       │
       ▼
┌──────────────┐
│ Rust Consumer│
│ (Async, Fast)│
└──────────────┘
```

---

## Key Features

- **High Throughput**
  - RabbitMQ Streams optimized for append-only workloads

- **Low Latency**
  - Zero-copy decoding in Rust

- **Polyglot**
  - Go for ergonomics
  - Rust for performance and safety

- **Schema-Driven**
  - Protocol Buffers shared across languages

- **Type-Safe Task Routing**
  - Task types are defined as enums, not strings

- **Replay & Recovery**
  - Stream offsets allow replay and debugging

- **Batch Operations**
  - Supports batched account and transfer creation

---

## Technology Stack

| Component     | Technology                                        |
| ------------- | ------------------------------------------------- |
| Messaging     | RabbitMQ Streams                                  |
| Serialization | Protocol Buffers (proto3)                         |
| Producer      | Golang (`rabbitmq-stream-go-client`)              |
| Consumer      | Rust (`rabbitmq-stream-client`, `prost`, `tonic`) |
| Logging       | zap (Go), tracing (Rust)                          |

---

## Message Schema (Protobuf)

### TaskRequest (Flattened)

```protobuf
message TaskRequest {
  string id = 1;
  TaskType task_type = 2;
  int64 created_at = 3;

  optional uint32 priority = 4;
  optional int32 retry_count = 5;

  optional uint32 ledger = 10;
  optional uint32 code = 11;
  optional uint32 flags = 12;

  optional UInt128 account_id = 20;
  optional UInt128 transfer_id = 21;
  optional UInt128 debit_account_id = 22;
  optional UInt128 credit_account_id = 23;

  optional UInt128 amount = 30;

  optional UInt128 user_data_128 = 40;
  optional uint64 user_data_64 = 41;
  optional uint32 user_data_32 = 42;

  repeated CreateAccountRequest account_batch = 50;
  repeated CreateTransferRequest transfer_batch = 51;
  repeated UInt128 lookup_ids = 52;
}
```

---

### TaskType Enum

```protobuf
enum TaskType {
  TASK_TYPE_UNSPECIFIED = 0;
  CREATE_ACCOUNT = 1;
  BATCH_ACCOUNTS = 2;
  LOOKUP_ACCOUNTS = 3;

  CREATE_TRANSFER = 4;
  BATCH_TRANSFERS = 5;
  LOOKUP_TRANSFERS = 6;
}
```

---

### UInt128 Helper Type

Used to safely transport 128-bit identifiers across languages.

```protobuf
message UInt128 {
  fixed64 low = 1;
  fixed64 high = 2;
}
```

Defaults:

- `low = 0`
- `high = 0`

---

## JSON Compatibility

The Go producer accepts **JSON input** and converts it into Protobuf.

### Example: Create Account

```json
{
  "id": "task-123",
  "taskType": 1,
  "createdAt": 1766864710,
  "ledger": 1,
  "code": 100,
  "accountId": {
    "low": 1239,
    "high": 0
  },
  "userData128": {
    "low": 9871,
    "high": 0
  },
  "userData64": 5555,
  "userData32": 42
}
```

> ⚠️ **Important**
>
> - Enum values **must be uppercase**
> - All `UInt128` fields must include **both `low` and `high`**

---

## Producer (Go)

- Reads JSON task definitions
- Validates required fields
- Converts to Protobuf
- Publishes to RabbitMQ Streams
- Supports:
  - JSON publish
  - Protobuf publish
  - Publisher confirms

---

## Consumer (Rust)

- Decodes JSON or Protobuf automatically
- Routes tasks via `TaskType` enum
- Uses async processing
- Handles:
  - Account creation
  - Transfers
  - Batch operations
  - Lookups

---

## Reliability & Safety

- **Strong typing** across producer and consumer
- **No string-based routing**
- **Backpressure support**
- **At-least-once delivery**
- **Idempotent task handling ready**

---

## Roadmap

- [x] RabbitMQ Streams integration
- [x] Protobuf schema unification
- [x] Enum-based task routing
- [x] TigerBeetle database integration
- [ ] TigerBeetle CDC job integration
- [ ] Metrics & observability (Prometheus / Grafana)
- [ ] Dead-letter stream support
- [ ] Consumer health checks
- [ ] TLS & authentication
- [ ] CI/CD pipeline
- [ ] Task retry & backoff policies `
