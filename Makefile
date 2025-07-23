# Makefile for sammy_monitor

.PHONY: help build test run clean docker-build docker-run docker-compose-up docker-compose-down fmt clippy check

# Default target
help:
	@echo "Available targets:"
	@echo "  build           - Build the project in release mode"
	@echo "  test            - Run all tests"
	@echo "  run             - Run the application with default settings"
	@echo "  dev             - Run in development mode with debug logging"
	@echo "  clean           - Clean build artifacts"
	@echo "  check           - Run cargo check"
	@echo "  fmt             - Format code"
	@echo "  clippy          - Run clippy lints"
	@echo "  docker-build    - Build Docker image (Alpine-based)"
	@echo "  docker-run      - Run in Docker container"
	@echo "  compose-up      - Start with docker-compose"
	@echo "  compose-down    - Stop docker-compose services"

# Rust targets
build:
	cargo build --release

test:
	cargo test

run:
	cargo run --release -- --settings ./settings.toml

dev:
	RUST_LOG=debug cargo run -- --settings ./settings.toml

clean:
	cargo clean

check:
	cargo check

fmt:
	cargo fmt --all

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

# Docker targets
docker-build:
	./build.sh

docker-run: docker-build
	docker run -d \
		--name sammy_monitor \
		-p 3000:3000 \
		-p 3001:3001 \
		-v $(PWD)/settings.toml:/app/settings.toml:ro \
		sammy_monitor:latest

compose-up:
	docker-compose up -d

compose-down:
	docker-compose down

compose-logs:
	docker-compose logs -f sammy_monitor

# Development setup
setup:
	@if [ ! -f settings.toml ]; then \
		cp settings.sample.toml settings.toml; \
		echo "Created settings.toml from sample. Please edit it with your monitors."; \
	fi

# Install development dependencies
install-deps:
	cargo install cargo-audit cargo-llvm-cov

# Run security audit
audit:
	cargo audit

# Generate code coverage
coverage:
	cargo llvm-cov --all-features --workspace --html
