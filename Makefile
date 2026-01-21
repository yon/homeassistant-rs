# Home Assistant Rust - Makefile
#
# Run `make help` to see all available targets.

# Python virtual environment
# PYTHON_BIN: System Python used to create the venv
# PYTHON: Python inside the venv (used after venv exists)
PYTHON_BIN := $(shell command -v python3.13 2>/dev/null || command -v python3)
VENV := .venv
VENV_BIN := $(VENV)/bin
PYTHON := $(VENV_BIN)/python
MATURIN := $(VENV_BIN)/maturin
VENV_STAMP := $(VENV)/.stamp

# Configuration directory (optional, uses server default if not set)
CONFIG_DIR ?=

# Common environment for run targets
SITE_PACKAGES = $(shell $(PYTHON) -c "import site; print(site.getsitepackages()[0])")
RUN_ENV = PYTHONPATH=$(CURDIR)/crates/ha-py-bridge/python:$(SITE_PACKAGES) \
	HA_FRONTEND_PATH=$(SITE_PACKAGES)/hass_frontend \
	PYO3_PYTHON=$(CURDIR)/$(PYTHON) \
	$(if $(CONFIG_DIR),HA_CONFIG_DIR=$(CONFIG_DIR))

# Default target
.DEFAULT_GOAL := help

##@ Build

.PHONY: build
build: ## Build all crates in debug mode
	cargo build --workspace

.PHONY: build-release
build-release: ## Build all crates in release mode
	cargo build --workspace --release

.PHONY: build-wheel
build-wheel: $(VENV_STAMP) ## Build Python wheel (Mode 1: extension)
	cd crates/ha-py-bridge && $(CURDIR)/$(MATURIN) build --release

.PHONY: build-wheel-debug
build-wheel-debug: $(VENV_STAMP) ## Build Python wheel in debug mode
	cd crates/ha-py-bridge && $(CURDIR)/$(MATURIN) build

##@ Check & Lint

.PHONY: check
check: ## Check all crates for errors without building
	cargo check --workspace

.PHONY: clippy
clippy: ## Run clippy linter on all crates
	cargo clippy --workspace --all-targets -- -D warnings

.PHONY: fmt
fmt: ## Format all code with rustfmt
	cargo fmt --all

.PHONY: fmt-check
fmt-check: ## Check if code is formatted correctly
	cargo fmt --all -- --check

.PHONY: lint
lint: fmt-check clippy lint-makefile ## Run all linters

.PHONY: lint-makefile
lint-makefile: ## Check Makefile targets are alphabetized within sections
	@awk '/^##@/ { section=$$0; delete targets; n=0 } \
	     /^\.PHONY:/ { target=$$2; if (n>0 && target<targets[n]) { \
	         print "Error: " target " should come before " targets[n] " in section: " section; \
	         exit 1 } \
	       n++; targets[n]=target }' $(MAKEFILE_LIST) && echo "Makefile targets are alphabetized"

##@ Development

.PHONY: clean
clean: ## Remove build artifacts
	cargo clean

.PHONY: clean-all
clean-all: clean ## Remove build artifacts and Python venv
	rm -rf $(VENV)

.PHONY: dev
dev: fmt clippy test ## Run all development checks (format, lint, test)

.PHONY: install-dev
install-dev: $(VENV_STAMP) ## Install Python extension in development mode
	cd crates/ha-py-bridge && $(CURDIR)/$(MATURIN) develop

.PHONY: run
run: $(VENV_STAMP) ## Run the Home Assistant server (strict mode - no native fallback)
	$(RUN_ENV) cargo run --bin homeassistant --features python

.PHONY: run-fallback
run-fallback: $(VENV_STAMP) ## Run with native HA fallback enabled (development only)
	ALLOW_HA_NATIVE_FALLBACK=1 $(RUN_ENV) cargo run --bin homeassistant --features python

.PHONY: run-release
run-release: $(VENV_STAMP) ## Run the Home Assistant server in release mode (strict)
	$(RUN_ENV) cargo run --bin homeassistant --features python --release

.PHONY: setup
setup: $(VENV_STAMP) ## Setup development environment (git hooks, venv)
	git config core.hooksPath .githooks
	@echo "Git hooks configured. Pre-commit will run fmt, clippy, and tests."

.PHONY: watch
watch: ## Watch for changes and rebuild (requires cargo-watch)
	cargo watch -x 'build --workspace'

##@ Documentation

.PHONY: doc
doc: ## Generate documentation for all crates
	cargo doc --workspace --no-deps

.PHONY: doc-open
doc-open: ## Generate and open documentation in browser
	cargo doc --workspace --no-deps --open

##@ Help

.PHONY: help
help: ## Display this help message
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Python

.PHONY: setup-venv
setup-venv: $(VENV_STAMP) ## Create Python virtual environment with tools

##@ HA Test Environment
# Setup and manage HA test instances

.PHONY: ha-install-deps
ha-install-deps: $(VENV_STAMP) ## Install Home Assistant Python dependencies
	$(VENV_BIN)/pip install -c vendor/ha-core/homeassistant/package_constraints.txt \
		-r vendor/ha-core/requirements_all.txt

.PHONY: ha-setup
ha-setup: ha-install-deps ## Setup HA compatibility test environment
	./tests/ha_compat/setup.sh

.PHONY: ha-start
ha-start: ## Start HA test instance in Docker
	$(MAKE) -f tests/Makefile ha-start

.PHONY: ha-status
ha-status: ## Check status of HA test instance
	$(MAKE) -f tests/Makefile ha-status

.PHONY: ha-stop
ha-stop: ## Stop HA test instance
	$(MAKE) -f tests/Makefile ha-stop

##@ Testing

.PHONY: test
test: test-rust test-python test-integration test-ha-compat ## Run ALL tests (Rust + Python + integration)

.PHONY: test-compare
test-compare: ## Run API comparison tests against Python HA (requires Docker)
	$(MAKE) -f tests/Makefile compare

.PHONY: test-ha-compat
test-ha-compat: install-dev ## Run HA test suite with Rust extension
	$(PYTHON) tests/ha_compat/run_tests.py --all -v

.PHONY: test-integration
test-integration: build $(VENV_STAMP) ## Run WebSocket API integration tests (starts Rust server automatically)
	$(VENV_BIN)/pytest tests/integration/ -v

.PHONY: test-python
test-python: install-dev ## Run all Python tests (shim + PyO3 extension)
	$(VENV_BIN)/pytest crates/ha-py-bridge/python/tests/ -v

.PHONY: test-rust
test-rust: $(VENV_STAMP) ## Run all Rust tests
	$(RUN_ENV) cargo test --workspace --exclude ha-py-bridge
	$(RUN_ENV) cargo test -p ha-automation --test compat_test
	$(RUN_ENV) cargo test -p ha-script --test compat_test

##@ Utilities

.PHONY: audit
audit: ## Run security audit (requires cargo-audit)
	cargo audit

.PHONY: deps
deps: ## Check for outdated dependencies (requires cargo-outdated)
	cargo outdated --workspace

.PHONY: tree
tree: ## Display dependency tree
	cargo tree --workspace

.PHONY: update
update: ## Update dependencies
	cargo update

# Internal targets (not shown in help)

$(VENV_STAMP):
	$(PYTHON_BIN) -m venv $(VENV)
	$(VENV_BIN)/pip install --upgrade pip
	$(VENV_BIN)/pip install maturin pytest pytest-asyncio aiohttp
	touch $(VENV_STAMP)
