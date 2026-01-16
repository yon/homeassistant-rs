# Home Assistant Rust - Makefile
#
# Run `make help` to see all available targets.

CARGO := cargo
RUSTFMT := rustfmt
CLIPPY := cargo clippy

# Default target
.DEFAULT_GOAL := help

##@ Build

.PHONY: build
build: ## Build all crates in debug mode
	$(CARGO) build --workspace

.PHONY: build-release
build-release: ## Build all crates in release mode
	$(CARGO) build --workspace --release

##@ Check & Lint

.PHONY: check
check: ## Check all crates for errors without building
	$(CARGO) check --workspace

.PHONY: clippy
clippy: ## Run clippy linter on all crates
	$(CLIPPY) --workspace --all-targets -- -D warnings

.PHONY: fmt
fmt: ## Format all code with rustfmt
	$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check: ## Check if code is formatted correctly
	$(CARGO) fmt --all -- --check

##@ Documentation

.PHONY: doc
doc: ## Generate documentation for all crates
	$(CARGO) doc --workspace --no-deps

.PHONY: doc-open
doc-open: ## Generate and open documentation in browser
	$(CARGO) doc --workspace --no-deps --open

##@ Development

.PHONY: clean
clean: ## Remove build artifacts
	$(CARGO) clean

.PHONY: dev
dev: fmt clippy test ## Run all development checks (format, lint, test)

.PHONY: run
run: ## Run the Home Assistant server
	$(CARGO) run --bin homeassistant

.PHONY: run-release
run-release: ## Run the Home Assistant server in release mode
	$(CARGO) run --bin homeassistant --release

.PHONY: watch
watch: ## Watch for changes and rebuild (requires cargo-watch)
	$(CARGO) watch -x 'build --workspace'

##@ Help

.PHONY: help
help: ## Display this help message
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Testing

.PHONY: test
test: ## Run all tests
	$(CARGO) test --workspace

.PHONY: test-coverage
test-coverage: ## Run tests with coverage (requires cargo-tarpaulin)
	$(CARGO) tarpaulin --workspace --out Html --output-dir target/coverage

.PHONY: test-doc
test-doc: ## Run documentation tests
	$(CARGO) test --workspace --doc

.PHONY: test-verbose
test-verbose: ## Run all tests with verbose output
	$(CARGO) test --workspace -- --nocapture

##@ Utilities

.PHONY: audit
audit: ## Run security audit (requires cargo-audit)
	$(CARGO) audit

.PHONY: deps
deps: ## Check for outdated dependencies (requires cargo-outdated)
	$(CARGO) outdated --workspace

.PHONY: tree
tree: ## Display dependency tree
	$(CARGO) tree --workspace

.PHONY: update
update: ## Update dependencies
	$(CARGO) update
