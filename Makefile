# Ridge — Makefile

.PHONY: build test clippy fmt check run install clean help

# Default Rust toolchain (overridable via environment)
CARGO ?= cargo
RUSTFLAGS ?=

help: ## Show this help message
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## Build the project in debug mode
	$(CARGO) build

release: ## Build the project in release mode
	$(CARGO) build --release

test: ## Run all tests
	$(CARGO) test

clippy: ## Run clippy lints
	$(CARGO) clippy -- -D warnings

fmt: ## Format code with rustfmt
	$(CARGO) fmt

fmt-check: ## Check code formatting without modifying files
	$(CARGO) fmt --check

check: fmt-check clippy test ## Run all checks (fmt, clippy, test)

run: build ## Build and run the binary
	$(CARGO) run

install: ## Install ridge to ~/.cargo/bin
	$(CARGO) install --path .

clean: ## Remove build artifacts
	$(CARGO) clean
	rm -rf target/

all: build test clippy fmt-check ## Build, test, lint, and format-check
