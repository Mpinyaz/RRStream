# RabbitMQ Stream Task System

A high-performance, distributed task processing system built on **RabbitMQ
Streams**, with a **Golang producer**, a **Rust consumer**, and a **gRPC-based
submission API**, using **Protocol Buffers** for strict schema and type safety.
The system leverages the **TigerBeetle financial database** for storing
financial records with exceptional throughput and strong correctness guarantees.

## Overview

This project implements a **polyglot, stream-based task processing
architecture** optimized for **throughput, low latency, and correctness** in
financial operations.

The system supports **synchronous task submission via gRPC**, while execution
remains **asynchronous, durable, and replayable** via RabbitMQ Streams.

### Key Idea

- **gRPC** is used for request/response semantics (task submission + responses)
- **RabbitMQ Streams** is the durable execution backbone
- **Rust consumers** execute tasks asynchronously and at scale
- **TigerBeetle** guarantees ACID financial correctness

## Why This Stack?

### Go Producer

- Excellent ergonomics for API development
- Strong validation and schema enforcement
- Natural fit for gRPC servers
- JSON → Protobuf conversion for REST (optional)
- Publishes validated tasks to RabbitMQ Streams
- Sends execution responses back via gRPC

### Rust Consumer

- Zero-cost abstractions and memory safety
- High-performance async execution with Tokio
- Enum-based task routing
- Direct TigerBeetle integration
- Acts as a **gRPC client** to publish task responses

### RabbitMQ Streams

- Append-only log semantics
- Message replay and consumer offsets
- Horizontal scaling via consumer groups
- Ideal for financial workloads and recovery

### Protocol Buffers

- Cross-language schema consistency
- Backward/forward compatibility
- Compact binary encoding
- Single source of truth for API + messaging

### TigerBeetle

- ACID-compliant financial ledger
- Double-entry accounting
- Extremely high throughput
- Immutable audit log

## Architecture

┌─────────────┐ │ Clients │ │ (Rust / CLI │ │ / Services)│ └──────┬──────┘ │
gRPC (SubmitTask) ▼ ┌──────────────────────────┐ │ Go gRPC Server │ │
┌────────────────────┐ │ │ │ Validation Layer │ │ │ └─────────┬──────────┘ │ │ │
│ │ ┌─────────▼──────────┐ │ │ │ Protobuf Encoding │ │ │ └─────────┬──────────┘
│ │ │ │ │ ┌─────────▼──────────┐ │ │ │ RabbitMQ Stream │ │ │ │ Producer │ │ │
└────────────────────┘ │ └──────────┬───────────────┘ │ Protobuf Messages ▼
┌────────────────────────┐ │ RabbitMQ Streams │ │ ┌──────────────────┐ │ │ │
Persistent Log │ │ │ └──────────────────┘ │ └──────────┬─────────────┘ │ ▼
┌──────────────────────────┐ │ Rust Consumer Pool │ │ ┌────────────────────┐ │ │
│ Task Router │ │ │ └─────────┬──────────┘ │ │ │ │ │ ┌─────────▼──────────┐ │ │
│ TigerBeetle Client │ │ │ └─────────┬──────────┘ │ │ │ │ │
┌─────────▼──────────┐ │ │ │ gRPC Client │ │ │ └────────────────────┘ │
└──────────────────────────┘

## Data Flow

1. **Ingestion** Clients submit financial operations as JSON to the Go producer.

2. **Validation** The producer validates schemas, checks business rules, and
   enforces rate limits.

3. **Serialization** JSON payloads are converted into a compact Protocol Buffers
   (Protobuf) format.

4. **Publishing** Messages are written to RabbitMQ Streams with publisher
   confirms enabled.

5. **Distribution** Stream consumers read messages using offset tracking.

6. **Processing** Rust workers decode, route, and execute tasks in parallel.

7. **Persistence** Financial operations are committed to TigerBeetle with ACID
   guarantees.

8. **Acknowledgment** Successful processing updates consumer offsets.

## Key Features

### Performance Characteristics

#### High Throughput

- RabbitMQ Streams optimized for append-only workloads with minimal disk seeks
- TigerBeetle processes **1M+ transactions/second** on commodity hardware
- Batch operations amortize network and disk I/O costs
- Zero-copy decoding in Rust eliminates allocation overhead

#### Low Latency

- End-to-end **p99 latency under 10ms** for single operations
- Async processing prevents blocking on I/O
- Direct memory access patterns in Rust consumer
- Efficient Protobuf encoding reduces serialization time

### Reliability & Correctness

#### Polyglot Architecture

- Go for ergonomic API development and rapid iteration
- Rust for performance-critical consumer path and memory safety
- Shared Protobuf schema eliminates serialization mismatches

#### Schema-Driven Development

- Protocol Buffers provide a single source of truth
- Code generation ensures type safety across languages
- Breaking changes are caught at compile time
- Optional fields enable backward-compatible evolution

#### Type-Safe Task Routing

- Task types defined as enums, not error-prone strings
- Exhaustive pattern matching in Rust catches unhandled cases
- Compiler enforces handling of all task variants
- Impossible to route to non-existent handlers

#### Replay & Recovery

- Stream offsets allow precise replay from any point
- Debug production issues by replaying message sequences
- Disaster recovery through message log reconstruction
- Consumer can restart from last committed offset

#### Idempotency Support

- Task IDs enable duplicate detection
- TigerBeetle’s natural idempotency for financial operations
- Consumer can safely retry failed operations
- At-least-once delivery with exactly-once semantics

### Operational Features

#### Batch Operations

- Create hundreds of accounts in a single request
- Process transfer batches with atomicity guarantees
- Reduce network round-trips by **100×**
- TigerBeetle’s native batch API for maximum throughput

#### Observability

- Structured logging in both producer and consumer
- OpenTelemetry-compatible tracing _(planned)_
- Prometheus metrics for monitoring _(planned)_
- Consumer lag tracking for capacity planning

#### Horizontal Scaling

- Add consumer instances without coordination
- RabbitMQ Stream consumer groups for load balancing
- Partitioning support for parallel processing
- TigerBeetle cluster scales to multiple nodes

## Protobuf Schema

### TaskType Enum

```proto
enum TaskType {
  UNKNOWN = 0;
  CREATE_ACCOUNT = 1;
  BATCH_ACCOUNTS = 2;
  LOOKUP_ACCOUNTS = 3;
  CREATE_TRANSFER = 4;
  BATCH_TRANSFERS = 5;
  LOOKUP_TRANSFERS = 6;
}
```

### UInt128

```proto
message UInt128 {
  fixed64 low = 1;
  fixed64 high = 2;
}
```

### TaskRequest

```proto
syntax = "proto3";

package tasks;

// Unified task request supporting all financial operations
message TaskRequest {
  // === Core Task Metadata ===
  string id = 1;                      // Unique task identifier (UUID recommended)
  TaskType task_type = 2;             // Determines which fields are relevant
  int64 created_at = 3;               // Unix timestamp (seconds since epoch)
  optional uint32 priority = 4;       // Task priority (higher = more urgent)
  optional int32 retry_count = 5;     // Number of retry attempts

  // === TigerBeetle Account/Transfer Common Fields ===
  optional uint32 ledger = 10;        // Ledger identifier for grouping
  optional uint32 code = 11;          // User-defined code (e.g., account type)
  optional uint32 flags = 12;         // TigerBeetle flags (linked, pending, etc.)

  // === Account Operations ===
  optional UInt128 account_id = 20;   // For CREATE_ACCOUNT, LOOKUP_ACCOUNTS

  // === Transfer Operations ===
  optional UInt128 transfer_id = 21;         // For CREATE_TRANSFER, LOOKUP_TRANSFERS
  optional UInt128 debit_account_id = 22;    // Source account
  optional UInt128 credit_account_id = 23;   // Destination account
  optional UInt128 amount = 30;              // Transfer amount (128-bit precision)

  // === User-Defined Metadata ===
  optional UInt128 user_data_128 = 40;  // 128-bit custom data
  optional uint64 user_data_64 = 41;    // 64-bit custom data
  optional uint32 user_data_32 = 42;    // 32-bit custom data

  // === Batch Operations ===
  repeated CreateAccountRequest account_batch = 50;   // Batch account creation
  repeated CreateTransferRequest transfer_batch = 51; // Batch transfer creation
  repeated UInt128 lookup_ids = 52;                   // IDs for lookup operations
}

// Supporting message for batch account creation
message CreateAccountRequest {
  UInt128 id = 1;
  uint32 ledger = 2;
  uint32 code = 3;
  optional uint32 flags = 4;
  optional UInt128 user_data_128 = 5;
  optional uint64 user_data_64 = 6;
  optional uint32 user_data_32 = 7;
}

// Supporting message for batch transfer creation
message CreateTransferRequest {
  UInt128 id = 1;
  UInt128 debit_account_id = 2;
  UInt128 credit_account_id = 3;
  UInt128 amount = 4;
  uint32 ledger = 5;
  uint32 code = 6;
  optional uint32 flags = 7;
  optional UInt128 user_data_128 = 8;
  optional uint64 user_data_64 = 9;
  optional uint32 user_data_32 = 10;
}
```

## JSON Rules

- Enums must be numeric (`taskType: 1`)
- UInt128 requires both `low` and `high`
- Required fields: `id`, `taskType`, `createdAt`

## Producer (Go)

The Go producer serves as the **validation and ingestion layer**, ensuring only
valid, well-formed tasks enter the system.

### Responsibilities

#### Input Validation

- Verify required fields are present for each task type
- Validate `UInt128` structures have both `low` and `high` fields
- Check enum values are within the valid range
- Enforce business rules (e.g., transfer amount > 0)

#### Format Conversion

- Parse JSON from HTTP requests or CLI input
- Convert data to Protobuf binary encoding
- Handle field name translations (`camelCase` → `snake_case`)

#### Stream Publishing

- Connect to RabbitMQ Streams
- Publish messages with publisher confirms for durability
- Handle backpressure and connection failures
- Support batched publishing for high throughput

#### Error Handling

- Return descriptive validation errors to clients
- Log publishing failures using structured logging
- Retry transient failures with exponential backoff

## Consumer (Rust)

The Rust consumer is the **performance-critical execution layer**, responsible
for decoding, routing, and executing financial operations with maximum
throughput and safety.

### Responsibilities

#### Message Decoding

- Deserialize Protobuf binary data
- Validate message integrity
- Handle both JSON and Protobuf formats
- Perform zero-copy parsing where possible

#### Task Routing

- Match on `TaskType` enum
- Extract relevant fields for each operation type
- Validate operation-specific requirements
- Dispatch to the appropriate handler

#### TigerBeetle Integration

- Create accounts with proper error handling
- Execute transfers atomically
- Handle batch operations efficiently
- Process lookup queries

#### Error Recovery

- Retry transient failures
- Log permanent failures for investigation
- Update consumer offsets correctly
- Maintain exactly-once semantics where possible

## Technology Stack

| Component       | Technology                                 | Purpose                                         |
| --------------- | ------------------------------------------ | ----------------------------------------------- |
| Messaging       | RabbitMQ Streams 3.13+                     | Durable, replayable message log                 |
| Serialization   | Protocol Buffers (proto3)                  | Cross-language type-safe encoding               |
| Producer        | Golang 1.21+ (rabbitmq-stream-go-client)   | API layer, validation, publishing               |
| Consumer        | Rust 1.75+ (rabbitmq-stream-client, prost) | High-performance task execution                 |
| Financial DB    | TigerBeetle 0.15+                          | ACID-compliant financial ledger                 |
| Logging (Go)    | zap                                        | Structured logging with performance             |
| Logging (Rust)  | tracing + tracing-subscriber               | Async-aware structured logging                  |
| Build (Go)      | go mod                                     | Dependency management                           |
| Build (Rust)    | Cargo                                      | Build system and package manager                |
| Code Generation | protoc, prost-build                        | Generate type-safe bindings from `.proto` files |

## Summary

A production-grade, replayable, type-safe financial task processing pipeline
built for extreme scale and correctness.
