.PHONY: help build-bpf build test clean deploy

help:
	@echo "Percolator v0 Build Targets"
	@echo ""
	@echo "  make build-bpf    - Build all programs for Solana BPF"
	@echo "  make build        - Build all programs (native)"
	@echo "  make test         - Run all unit tests"
	@echo "  make clean        - Clean build artifacts"
	@echo "  make deploy       - Deploy programs to localnet"
	@echo ""

build-bpf:
	@echo "Building BPF programs..."
	@cargo build-sbf

build:
	@echo "Building native..."
	@cargo build --lib --all

test:
	@echo "Running tests..."
	@cargo test --lib

clean:
	@echo "Cleaning..."
	@cargo clean

deploy:
	@echo "Deploying to localnet..."
	@echo "TODO: Implement deployment script"
