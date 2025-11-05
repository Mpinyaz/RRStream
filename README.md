# RabbitMQ Stream Task System

A high-performance, distributed task processing system built with RabbitMQ
Streams, featuring a Golang producer and Rust consumer implementation.

## Overview

This project implements a polyglot message broker architecture that leverages
RabbitMQ Streams for high-throughput, low-latency task processing. The system is
designed with:

- **Producer (Go)**: Handles incoming requests and publishes tasks to RabbitMQ
  Streams
- **Consumer (Rust)**: Processes tasks from the stream with high performance and
  reliability

## Architecture

```
┌─────────────┐         ┌──────────────┐         ┌─────────────┐
│   Clients   │────────▶│  Go Producer │────────▶│  RabbitMQ   │
└─────────────┘         └──────────────┘         │   Stream    │
                                                 └──────┬──────┘
                                                        │
                                                        ▼
                                                 ┌─────────────┐
                                                 │   Rust      │
                                                 │  Consumer   │
                                                 └─────────────┘
```

- **High Throughput**: RabbitMQ Streams provide exceptional message throughput
  for demanding workloads
- **Low Latency**: Stream-based architecture minimizes processing delays
- **Polyglot Design**: Combines Go's simplicity for producers with Rust's
  performance for consumers
- **Scalability**: Easily scale producers and consumers independently
- **Reliability**: Built-in message persistence and replay capabilities
- **Type Safety**: Protocol Buffers ensure consistent message schemas across
  languages

## Technology Stack

- **RabbitMQ Streams**: Message streaming platform
- **Protocol Buffers**: Efficient binary serialization format
- **Golang**: Producer implementation with `rabbitmq-stream-go-client`
- **Rust**: Consumer implementation with `rabbitmq-stream-client` and `prost`

## Message Schema

The system uses Protocol Buffers for message serialization:

```protobuf
message Task {
  string id = 1;
  string task_type = 2;
  map payload = 3;
  int64 created_at = 4;
  optional uint32 priority = 5;
  optional int32 retry_count = 6;
}
```

## Roadmap

- [x] Create Docker Compose setup
- [ ] Add metrics and observability (Prometheus/Grafana)
- [ ] Implement dead letter queue handling
- [ ] Add CI/CD pipeline
- [ ] Add authentication and TLS support
- [ ] Implement consumer health checks
