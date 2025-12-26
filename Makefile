# 1. Load Environment Variables from .env
ifneq ("$(wildcard .env)","")
    include .env
    export $(shell sed 's/=.*//' .env)
endif

.PHONY: setup-db start-db setup-rabbitmq start-cdc clean help

# --- Configuration ---
CLUSTER_ID ?= 0
REPLICA_ID ?= 0
TB_ADDRESSES ?= 3000
DB_DIR = ./db
DATA_FILE = $(DB_DIR)/0_0.tigerbeetle

# RabbitMQ Mappings from your .env
RMQ_HOST = $(RABBITMQ_ADVERTISED_HOST)
RMQ_USER = $(RABBITMQ_DEFAULT_USER)
RMQ_PASS = $(RABBITMQ_DEFAULT_PASS)
RMQ_API_URL = http://$(RMQ_HOST):15672/api
EXCHANGE = tigerbeetle_exchange

# This must match your Rust format!("{}_responses", config.stream_name)
RES_STREAM = $(RABBITMQ_STREAM_NAME)_responses

# --- 1. TigerBeetle Database ---

setup-db: ## Format the TigerBeetle data file
	@mkdir -p $(DB_DIR)
	./bin/tigerbeetle format --cluster=$(CLUSTER_ID) --replica=$(REPLICA_ID) --replica-count=1 --development $(DATA_FILE)

start-db: ## Start the TigerBeetle database server
	./bin/tigerbeetle start --addresses=3000 --development $(DATA_FILE)

# --- 2. RabbitMQ Infrastructure ---

setup-rabbitmq: ## Setup the AMQP Exchange and Bind it to the Response Stream
	@echo "1. Creating CDC Fanout Exchange: $(EXCHANGE)"
	@curl -s -u $(RMQ_USER):$(RMQ_PASS) -X PUT -H "content-type:application/json" \
		$(RMQ_API_URL)/exchanges/%2f/$(EXCHANGE) -d '{"type":"fanout","durable":true}'

	@echo "\n2. Binding Response Stream to CDC Exchange..."
	@echo "Note: If the stream doesn't exist yet, run your Rust app once first."
	@curl -s -u $(RMQ_USER):$(RMQ_PASS) -X POST -H "content-type:application/json" \
		$(RMQ_API_URL)/bindings/%2f/e/$(EXCHANGE)/q/$(RES_STREAM) -d '{"routing_key":""}'
	@echo "\n✓ Infrastructure bridge created."

# --- 3. TigerBeetle CDC (Change Data Capture) ---

start-cdc: ## Start the CDC job (Bridge between TB and RMQ Exchange)
	./bin/tigerbeetle amqp \
		--addresses=$(TB_ADDRESSES) \
		--cluster=$(CLUSTER_ID) \
		--host=127.0.0.1 \
		--vhost=/ \
		--user=$(RMQ_USER) \
		--password=$(RMQ_PASS) \
		--publish-exchange=$(EXCHANGE)

# --- 4. Cleanup ---

clean: ## Wipe database and logs
	rm -rf $(DB_DIR)
	@echo "✓ Clean complete"

help: ## Show help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'
