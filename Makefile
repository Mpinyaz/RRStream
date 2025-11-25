.PHONY: proto clean help install-tools

# Generate protobuf Go code
proto:
	@echo "Generating protobuf files..."
	protoc \
		-I ./proto \
		--go_out=producer/pkg/models/proto \
		--go_opt=paths=source_relative \
		./proto/task.proto
	@echo "✓ Protobuf generation complete"

# Clean generated files
clean:
	@echo "Cleaning generated protobuf files..."
	rm -f producer/pkg/models/proto/*.pb.go
	@echo "✓ Clean complete"

# Install required tools
install-tools:
	@echo "Installing protoc tools..."
	go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
	@echo "✓ Tools installed"

# Help
help:
	@echo "Available targets:"
	@echo "  proto         - Generate Go code from .proto files"
	@echo "  clean         - Remove generated protobuf files"
	@echo "  install-tools - Install protoc plugin"
