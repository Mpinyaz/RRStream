
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

## API Gateway

The API Gateway exposes a REST and gRPC interface for clients to submit tasks securely and reliably.

### Features

- **gRPC Endpoint**: `SubmitTask(TaskRequest) → Empty`
- **REST Gateway** (optional via gRPC-Gateway):
  - POST `/tasks` with JSON body
  - Converts JSON to Protobuf
  - Validates input before publishing to RabbitMQ Streams
- **Authentication & Authorization**:
  - JWT or API keys
  - Enforces per-client rate limits
- **Observability**:
  - Logs incoming requests and validation errors
  - Metrics for request rates, latencies, and failures

### Architecture

```
Client → API Gateway → gRPC → Go Producer → RabbitMQ Streams
```

- The API Gateway **decouples clients** from internal message bus details.
- Supports **load balancing** across multiple Go producers.
- Allows **future extensions** for REST, GraphQL, or WebSocket interfaces.

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
