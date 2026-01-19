# Home Assistant Rust - Makefile
#
# Run `make help` to see all available targets.

CARGO := cargo
CLIPPY := cargo clippy
RUSTFMT := rustfmt

# Python virtual environment
# Prefer Homebrew Python 3.13, fall back to python3
PYTHON_BIN := $(shell command -v /opt/homebrew/opt/python@3.13/bin/python3.13 2>/dev/null || command -v python3)
VENV := .venv
VENV_BIN := $(VENV)/bin
PYTHON := $(VENV_BIN)/python
MATURIN := $(VENV_BIN)/maturin
VENV_STAMP := $(VENV)/.stamp

# Default target
.DEFAULT_GOAL := help

##@ Build

.PHONY: build
build: ## Build all crates in debug mode
	$(CARGO) build --workspace

.PHONY: build-release
build-release: ## Build all crates in release mode
	$(CARGO) build --workspace --release

.PHONY: build-wheel
build-wheel: $(VENV_STAMP) ## Build Python wheel (Mode 1: extension)
	$(MATURIN) build --release

.PHONY: build-wheel-debug
build-wheel-debug: $(VENV_STAMP) ## Build Python wheel in debug mode
	$(MATURIN) build

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
	$(CARGO) clean

.PHONY: clean-all
clean-all: clean ## Remove build artifacts and Python venv
	rm -rf $(VENV)

.PHONY: dev
dev: fmt clippy test ## Run all development checks (format, lint, test)

.PHONY: install-dev
install-dev: $(VENV_STAMP) ## Install Python extension in development mode
	$(MATURIN) develop

.PHONY: run
run: $(VENV_STAMP) ## Run the Home Assistant server (strict mode - no native fallback)
	PYTHONPATH=$(CURDIR)/python:$(shell $(PYTHON) -c "import site; print(site.getsitepackages()[0])") \
	HA_FRONTEND_PATH=$(shell $(PYTHON) -c "import site; print(site.getsitepackages()[0])")/hass_frontend \
	PYO3_PYTHON=$(CURDIR)/$(PYTHON) \
	$(CARGO) run --bin homeassistant --features python

.PHONY: run-fallback
run-fallback: $(VENV_STAMP) ## Run with native HA fallback enabled (development only)
	HA_ALLOW_NATIVE_FALLBACK=1 \
	PYTHONPATH=$(CURDIR)/python:$(shell $(PYTHON) -c "import site; print(site.getsitepackages()[0])") \
	HA_FRONTEND_PATH=$(shell $(PYTHON) -c "import site; print(site.getsitepackages()[0])")/hass_frontend \
	PYO3_PYTHON=$(CURDIR)/$(PYTHON) \
	$(CARGO) run --bin homeassistant --features python

.PHONY: run-release
run-release: $(VENV_STAMP) ## Run the Home Assistant server in release mode (strict)
	PYTHONPATH=$(CURDIR)/python:$(shell $(PYTHON) -c "import site; print(site.getsitepackages()[0])") \
	HA_FRONTEND_PATH=$(shell $(PYTHON) -c "import site; print(site.getsitepackages()[0])")/hass_frontend \
	PYO3_PYTHON=$(CURDIR)/$(PYTHON) \
	$(CARGO) run --bin homeassistant --features python --release

.PHONY: setup
setup: $(VENV_STAMP) ## Setup development environment (git hooks, venv)
	git config core.hooksPath .githooks
	@echo "Git hooks configured. Pre-commit will run fmt, clippy, and tests."

.PHONY: watch
watch: ## Watch for changes and rebuild (requires cargo-watch)
	$(CARGO) watch -x 'build --workspace'

##@ Documentation

.PHONY: doc
doc: ## Generate documentation for all crates
	$(CARGO) doc --workspace --no-deps

.PHONY: doc-open
doc-open: ## Generate and open documentation in browser
	$(CARGO) doc --workspace --no-deps --open

##@ Help

.PHONY: help
help: ## Display this help message
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Python

.PHONY: setup-venv
setup-venv: $(VENV_STAMP) ## Create Python virtual environment with tools

##@ HA Comparison Testing
# Detailed targets in tests/Makefile - these delegate to it

.PHONY: ha-compat-setup
ha-compat-setup: $(VENV_STAMP) ## Setup HA compatibility test environment
	./tests/ha_compat/setup.sh

.PHONY: ha-compat-test
ha-compat-test: install-dev ## Run HA test suite with Rust extension
	$(PYTHON) tests/ha_compat/run_tests.py --all -v

.PHONY: ha-start
ha-start: ## Start HA test instance in Docker
	$(MAKE) -f tests/Makefile ha-start

.PHONY: ha-status
ha-status: ## Check status of HA test instance
	$(MAKE) -f tests/Makefile ha-status

.PHONY: ha-stop
ha-stop: ## Stop HA test instance
	$(MAKE) -f tests/Makefile ha-stop

.PHONY: test-compare
test-compare: ## Run API comparison tests against Python HA
	$(MAKE) -f tests/Makefile compare

##@ Testing

.PHONY: test
test: test-rust test-python test-integration ## Run ALL tests (Rust + Python + integration)

.PHONY: test-coverage
test-coverage: ## Run Rust tests with coverage (requires cargo-tarpaulin)
	$(CARGO) tarpaulin --workspace --out Html --output-dir target/coverage

.PHONY: test-integration
test-integration: build $(VENV_STAMP) ## Run WebSocket API integration tests
	$(VENV_BIN)/pytest tests/integration/ -v

.PHONY: test-python
test-python: install-dev ## Run all Python tests (shim + PyO3 extension)
	$(VENV_BIN)/pytest python/tests/ crates/ha-core-rs/tests/python/ -v

.PHONY: test-rust
test-rust: ## Run all Rust tests
	$(CARGO) test --workspace --exclude ha-core-rs
	$(CARGO) test -p ha-automation --test compat_test
	$(CARGO) test -p ha-script --test compat_test

.PHONY: test-verbose
test-verbose: ## Run Rust tests with verbose output
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

# Internal targets (not shown in help)

$(VENV_STAMP):
	$(PYTHON_BIN) -m venv $(VENV)
	$(VENV_BIN)/pip install --upgrade pip
	$(VENV_BIN)/pip install maturin
	touch $(VENV_STAMP)
