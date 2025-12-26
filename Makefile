# 1. Load Environment Variables from .env
ifneq ("$(wildcard .env)","")
    include .env
    export $(shell sed 's/=.*//' .env)
endif

.PHONY: setup-db start-db setup-rabbitmq start-cdc clean clean-bin help install-db

# --- Configuration ---
CLUSTER_ID ?= 0
REPLICA_ID ?= 0
TB_ADDRESSES ?= 3000
DB_DIR = ./db
BIN_DIR = ./bin
TB_BIN = $(BIN_DIR)/tigerbeetle
DATA_FILE = $(DB_DIR)/0_0.tigerbeetle

# Detect OS for TigerBeetle download
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

ifeq ($(UNAME_S),Linux)
    ifeq ($(UNAME_M),x86_64)
        TB_URL = https://github.com/tigerbeetle/tigerbeetle/releases/latest/download/tigerbeetle-x86_64-linux.zip
    else ifeq ($(UNAME_M),aarch64)
        TB_URL = https://github.com/tigerbeetle/tigerbeetle/releases/latest/download/tigerbeetle-aarch64-linux.zip
    endif
else ifeq ($(UNAME_S),Darwin)
    ifeq ($(UNAME_M),arm64)
        TB_URL = https://github.com/tigerbeetle/tigerbeetle/releases/latest/download/tigerbeetle-aarch64-macos.zip
    else
        TB_URL = https://github.com/tigerbeetle/tigerbeetle/releases/latest/download/tigerbeetle-x86_64-macos.zip
    endif
endif

# RabbitMQ Mappings from your .env
RMQ_HOST = $(RABBITMQ_ADVERTISED_HOST)
RMQ_USER = $(RABBITMQ_DEFAULT_USER)
RMQ_PASS = $(RABBITMQ_DEFAULT_PASS)
RMQ_API_URL = http://$(RMQ_HOST):15672/api
EXCHANGE = $(TB_EXCHANGE)
RES_STREAM = $(RABBITMQ_STREAM_NAME)_responses

# --- 1. TigerBeetle Installation ---

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

# --- 2. TigerBeetle Database ---

setup-db: install-db ## Format the TigerBeetle data file
	@echo "🗄️  Setting up TigerBeetle database..."
	@mkdir -p $(DB_DIR)
	$(TB_BIN) format --cluster=$(CLUSTER_ID) --replica=$(REPLICA_ID) --replica-count=1 --development $(DATA_FILE)
	@echo "✅ Database formatted successfully"

start-db: ## Start the TigerBeetle database server
	@echo "🚀 Starting TigerBeetle server on port $(TB_ADDRESSES)..."
	$(TB_BIN) start --addresses=$(TB_ADDRESSES) --development $(DATA_FILE)

# --- 3. RabbitMQ Infrastructure ---

setup-rabbitmq: ## Setup the AMQP Exchange and Bind it to the Response Stream
	@echo "🐰 Setting up RabbitMQ infrastructure..."
	@echo "1️⃣  Creating CDC Fanout Exchange: $(EXCHANGE)"
	@curl -s -u $(RMQ_USER):$(RMQ_PASS) -X PUT -H "content-type:application/json" \
		$(RMQ_API_URL)/exchanges/%2f/$(EXCHANGE) -d '{"type":"fanout","durable":true}'
	@echo "\n2️⃣  Binding Response Stream to CDC Exchange..."
	@echo "   Note: If the stream doesn't exist yet, run your Rust app once first."
	@curl -s -u $(RMQ_USER):$(RMQ_PASS) -X POST -H "content-type:application/json" \
		$(RMQ_API_URL)/bindings/%2f/e/$(EXCHANGE)/q/$(RES_STREAM) -d '{"routing_key":""}'
	@echo "\n✅ Infrastructure bridge created."

# --- 4. TigerBeetle CDC (Change Data Capture) ---

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

# --- 5. Cleanup ---

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

# --- 6. Development Helpers ---

check-deps: ## Check if all dependencies are available
	@echo "🔍 Checking dependencies..."
	@command -v curl >/dev/null 2>&1 || { echo "❌ curl is required but not installed."; exit 1; }
	@command -v unzip >/dev/null 2>&1 || { echo "❌ unzip is required but not installed."; exit 1; }
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
