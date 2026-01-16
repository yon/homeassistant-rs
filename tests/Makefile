# Home Assistant Comparison Tests - Makefile
#
# This Makefile manages the HA comparison test infrastructure.
# Run from the repository root: make -f tests/Makefile <target>
# Or use the convenience targets in the root Makefile.

# Configuration
HA_VERSION ?= 2026.1.1
HA_IMAGE ?= ghcr.io/home-assistant/home-assistant:$(HA_VERSION)
HA_TEST_DIR := $(dir $(lastword $(MAKEFILE_LIST)))comparison
HA_CONFIG_DIR := $(HA_TEST_DIR)/ha-config
PYTHON_HA_PORT ?= 18123
RUST_HA_PORT ?= 18124
PYTHON_HA_URL := http://localhost:$(PYTHON_HA_PORT)
RUST_HA_URL := http://localhost:$(RUST_HA_PORT)
DOCKER_COMPOSE := docker compose -f $(HA_TEST_DIR)/docker-compose.yml

.DEFAULT_GOAL := help

##@ HA Test Instance

.PHONY: ha-clean
ha-clean: ## Remove HA test configuration (reset to fresh state)
	rm -rf $(HA_CONFIG_DIR)
	@echo "HA test configuration removed"

.PHONY: ha-logs
ha-logs: ## View logs from HA test instance
	$(DOCKER_COMPOSE) logs -f

.PHONY: ha-restart
ha-restart: ha-stop ha-start ## Restart HA test instance

.PHONY: ha-setup
ha-setup: ## Setup HA test instance configuration
	@echo "Setting up HA test instance (version $(HA_VERSION))..."
	$(HA_TEST_DIR)/setup-ha-test.sh $(HA_CONFIG_DIR)

.PHONY: ha-shell
ha-shell: ## Open a shell in the HA test container
	$(DOCKER_COMPOSE) exec homeassistant bash

.PHONY: ha-start
ha-start: ha-setup ## Start HA test instance in Docker
	@echo "Starting HA test instance ($(HA_IMAGE))..."
	HA_IMAGE=$(HA_IMAGE) HA_PORT=$(PYTHON_HA_PORT) $(DOCKER_COMPOSE) up -d
	@echo "Waiting for HA to be ready (this may take up to 2 minutes)..."
	@timeout 120 bash -c 'until curl -s -o /dev/null -w "%{http_code}" $(PYTHON_HA_URL)/api/ | grep -qE "^(200|401)$$"; do sleep 2; done' \
		|| (echo "HA failed to start"; $(DOCKER_COMPOSE) logs; exit 1)
	@echo "Generating API token..."
	@docker exec ha-test-instance python3 /config/generate-token.py > $(HA_CONFIG_DIR)/test-token.txt
	@echo "✓ HA test instance ready at $(PYTHON_HA_URL)"
	@echo "✓ API token saved to $(HA_CONFIG_DIR)/test-token.txt"

.PHONY: ha-status
ha-status: ## Check status of HA test instance
	@$(DOCKER_COMPOSE) ps
	@echo ""
	@HTTP_CODE=$$(curl -s -o /dev/null -w "%{http_code}" $(PYTHON_HA_URL)/api/); \
	if [ "$$HTTP_CODE" = "200" ] || [ "$$HTTP_CODE" = "401" ]; then \
		echo "✓ HA API is responding (HTTP $$HTTP_CODE)"; \
	else \
		echo "✗ HA API is not responding (HTTP $$HTTP_CODE)"; \
	fi

.PHONY: ha-stop
ha-stop: ## Stop HA test instance
	$(DOCKER_COMPOSE) down

.PHONY: ha-version
ha-version: ## Show configured HA version for testing
	@echo "Primary HA version: $(HA_VERSION)"
	@echo "Docker image: $(HA_IMAGE)"
	@echo ""
	@echo "Configured versions in ha-versions.toml:"
	@grep -E "^version|^docker_image" $(HA_TEST_DIR)/ha-versions.toml | head -10

##@ Comparison Tests

.PHONY: compare
compare: ## Run API comparison tests (requires both servers running)
	@echo "Running comparison tests..."
	@echo "Python HA: $(PYTHON_HA_URL)"
	@echo "Rust HA: $(RUST_HA_URL)"
	PYTHON_HA_URL=$(PYTHON_HA_URL) RUST_HA_URL=$(RUST_HA_URL) HA_VERSION=$(HA_VERSION) \
		cargo test -p ha-test-comparison -- --ignored --nocapture

.PHONY: compare-basic
compare-basic: ## Run only basic endpoint comparisons
	PYTHON_HA_URL=$(PYTHON_HA_URL) RUST_HA_URL=$(RUST_HA_URL) HA_VERSION=$(HA_VERSION) \
		cargo test -p ha-test-comparison test_basic_endpoints -- --ignored --nocapture

.PHONY: compare-ci
compare-ci: ha-start ## Full CI comparison (start HA, test, stop) - used by CI
	@echo "Starting Rust HA server..."
	HA_PORT=$(RUST_HA_PORT) cargo run --bin homeassistant &
	@sleep 5
	$(MAKE) compare || (pkill -f "target/debug/homeassistant" || true; $(MAKE) ha-stop; exit 1)
	@pkill -f "target/debug/homeassistant" || true
	$(MAKE) ha-stop

##@ Help

.PHONY: help
help: ## Display this help message
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make -f tests/Makefile \033[36m<target>\033[0m\n"} \
		/^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } \
		/^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)
