# 1. Load Environment Variables from .env
ifneq ("$(wildcard .env)","")
    include .env
    export $(shell sed 's/=.*//' .env)
endif

.PHONY: setup-db start-db setup-rabbitmq start-cdc clean clean-bin help install-db proto proto-go proto-rust build-producer build-all run-producer

# --- Configuration ---
APP_NAME ?= rrstreamer
CLUSTER_ID ?= 0
REPLICA_ID ?= 0
TB_ADDRESSES ?= 3000
DB_DIR = ./db
BIN_DIR = ./bin
TB_BIN = $(BIN_DIR)/tigerbeetle
DATA_FILE = $(DB_DIR)/0_0.tigerbeetle

# Project Structure
PRODUCER_DIR = ./producer
PROTO_DIR = ./proto
PROTO_FILE = $(PROTO_DIR)/task.proto
# This is the path relative to the root for cleanups/checks
PROTO_OUT_DIR = $(PRODUCER_DIR)/pkg/models/proto

# Detect OS for TigerBeetle download
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

ifeq ($(UNAME_S),Linux)
    ifeq ($(UNAME_M),x86_64)
        TB_URL = github.com
    else ifeq ($(UNAME_M),aarch64)
        TB_URL = github.com
    endif
else ifeq ($(UNAME_S),Darwin)
    ifeq ($(UNAME_M),arm64)
        TB_URL = github.com
    else
        TB_URL = github.com
    endif
endif

# RabbitMQ Mappings from .env
RMQ_HOST = $(RABBITMQ_ADVERTISED_HOST)
RMQ_USER = $(RABBITMQ_DEFAULT_USER)
RMQ_PASS = $(RABBITMQ_DEFAULT_PASS)
RMQ_API_URL = http://$(RMQ_HOST):15672/api
EXCHANGE = $(TB_EXCHANGE)
RES_STREAM = $(RABBITMQ_STREAM_NAME)_responses

# --- 1. Protobuf Generation ---

proto: proto-go proto-rust ## Generate protobuf code for both Go and Rust

proto-go: ## Generate Go protobuf code
	@echo "Generating Go protobuf files..."
	@mkdir -p $(PROTO_OUT_DIR)
	@protoc \
		-I ./proto \
		--go_out=$(PRODUCER_DIR) \
		--go_opt=paths=source_relative \
		--go-grpc_out=$(PRODUCER_DIR) \
		--go-grpc_opt=paths=source_relative \
		$(PROTO_FILE)
	@echo "✓ Go protobuf generation complete"

proto-rust: ## Generate Rust protobuf code
	@echo "🔨 Generating Rust protobuf code..."
	@cd ../consumer && cargo build
	@echo "✅ Rust protobuf code generated (via build.rs)"

# --- 2. TigerBeetle Installation ---

install-db: ## Download and install TigerBeetle binary
	@echo "🔍 Checking for existing TigerBeetle installation..."
	@if [ -f "$(TB_BIN)" ]; then \
		echo "✓ TigerBeetle already installed at $(TB_BIN)"; \
		$(TB_BIN) version; \
	else \
		echo "📦 Downloading TigerBeetle for $(UNAME_S)/$(UNAME_M)..."; \
		mkdir -p $(BIN_DIR); \
		curl -Lo /tmp/tigerbeetle.zip $(TB_URL); \
		echo "📂 Extracting..."; \
		unzip -o /tmp/tigerbeetle.zip -d $(BIN_DIR); \
		chmod +x $(TB_BIN); \
		rm -f /tmp/tigerbeetle.zip; \
		echo "✅ TigerBeetle installed successfully!"; \
		$(TB_BIN) version; \
	fi

# --- 3. TigerBeetle Database ---

setup-db: install-db ## Format the TigerBeetle data file
	@echo "🗄️ Setting up TigerBeetle database..."
	@mkdir -p $(DB_DIR)
	$(TB_BIN) format --cluster=$(CLUSTER_ID) --replica=$(REPLICA_ID) --replica-count=1 --development $(DATA_FILE)
	@echo "✅ Database formatted successfully"

start-db: ## Start the TigerBeetle database server
	@echo "🚀 Starting TigerBeetle server on port $(TB_ADDRESSES)..."
	$(TB_BIN) start --addresses=$(TB_ADDRESSES) --development $(DATA_FILE)

# --- 4. RabbitMQ Infrastructure ---

setup-rabbitmq: ## Setup the AMQP Exchange and Bind it to the Response Stream
	@echo "🐰 Setting up RabbitMQ infrastructure..."
	@echo "1️⃣ Creating CDC Fanout Exchange: $(EXCHANGE)"
	@curl -s -u $(RMQ_USER):$(RMQ_PASS) -X PUT -H "content-type:application/json" \
		$(RMQ_API_URL)/exchanges/%2f/$(EXCHANGE) -d '{"type":"fanout","durable":true}'
	@echo "\n2️⃣ Binding Response Stream to CDC Exchange..."
	@echo "   Note: If the stream doesn't exist yet, run your Rust app once first."
	@curl -s -u $(RMQ_USER):$(RMQ_PASS) -X POST -H "content-type:application/json" \
		$(RMQ_API_URL)/bindings/%2f/e/$(EXCHANGE)/q/$(RES_STREAM) -d '{"routing_key":""}'
	@echo "\n✅ Infrastructure bridge created."

# --- 5. TigerBeetle CDC (Change Data Capture) ---

start-cdc: install-db ## Start the CDC job (Bridge between TB and RMQ Exchange)
	@echo "🔄 Starting TigerBeetle CDC..."
	$(TB_BIN) amqp \
		--addresses=$(TB_ADDRESSES) \
		--cluster=$(CLUSTER_ID) \
		--host=127.0.0.1 \
		--vhost=/ \
		--user=$(RMQ_USER) \
		--password=$(RMQ_PASS) \
		--publish-exchange=$(EXCHANGE)

# --- 6. Cleanup ---

clean: ## Wipe database data files
	@echo "🧹 Cleaning database files..."
	rm -rf $(DB_DIR)
	@echo "✅ Database cleaned"

clean-bin: ## Remove TigerBeetle binary
	@echo "🧹 Removing TigerBeetle binary..."
	rm -rf $(BIN_DIR)
	@echo "✅ Binary removed"

clean-all: clean clean-bin ## Remove everything (database + binary)
	@echo "✅ Full cleanup complete"

# --- 7. Application Build & Run ---

build-producer: ## Build the Go producer binary
	@echo "🔨 Building Go producer..."
	@mkdir -p $(PRODUCER_DIR)/bin
	@# cd into producer directory so Go finds go.mod and local files correctly
	(cd $(PRODUCER_DIR) && go build -o bin/$(APP_NAME) main.go)
	@echo "✅ Producer built at $(PRODUCER_DIR)/bin/$(APP_NAME)"

run-producer: build-producer ## Build and run the Go producer
	@echo "🚀 Running producer..."
	@(cd $(PRODUCER_DIR) && ./bin/$(APP_NAME))

build-all: proto build-producer ## Build everything (Protobuf + Binaries)

# --- 8. Development Helpers ---

check-deps: ## Check if all dependencies are available
	@echo "🔍 Checking dependencies..."
	@command -v curl >/dev/null 2>&1 || { echo "❌ curl is required"; exit 1; }
	@command -v unzip >/dev/null 2>&1 || { echo "❌ unzip is required"; exit 1; }
	@command -v protoc >/dev/null 2>&1 || { echo "❌ protoc is required"; exit 1; }
	@command -v go >/dev/null 2>&1 || { echo "❌ go is required"; exit 1; }
	@echo "✅ All dependencies available"

status: ## Show status of TigerBeetle and database
	@echo "📊 System Status:"
	@echo "  TigerBeetle Binary: $(TB_BIN)"
	@if [ -f "$(TB_BIN)" ]; then \
		echo "  ✅ Installed: $$($(TB_BIN) version)"; \
	else \
		echo "  ❌ Not installed"; \
	fi
	@echo "  Database File: $(DATA_FILE)"
	@if [ -f "$(DATA_FILE)" ]; then \
		echo "  ✅ Exists ($$(du -h $(DATA_FILE) | cut -f1))"; \
	else \
		echo "  ❌ Not created"; \
	fi

help: ## Show this help message
	@echo "🎯 TigerBeetle + RabbitMQ CDC Setup"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "💡 Quick Start:"
	@echo "  make install-db     # Download TigerBeetle"
	@echo "  make setup-db       # Format database"
	@echo "  make start-db       # Start database server"
	@echo "  make setup-rabbitmq # Setup RabbitMQ bridge"
	@echo "  make start-cdc      # Start CDC"

.DEFAULT_GOAL := help
