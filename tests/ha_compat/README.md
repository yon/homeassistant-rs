# Home Assistant Compatibility Tests

This directory contains infrastructure for running Home Assistant's own test suite
against our Rust extension to verify API compatibility.

## Test Tiers

### Tier 1: Rust Compat Tests (Fast, No Python Required)

These are Rust tests that parse real HA config patterns to verify our types match.

```bash
make test-compat   # Run just compat tests
make test          # Includes compat tests
```

Test locations:
- `crates/ha-automation/tests/compat_test.rs` - Triggers and conditions
- `crates/ha-script/tests/compat_test.rs` - Actions

### Tier 2: HA Native Tests (Full Python HA Environment)

These run HA's actual pytest tests with our Rust components patched in.

```bash
# One-time setup
make ha-compat-setup

# Run all HA compat tests
make ha-compat-test

# Run specific category
python tests/ha_compat/run_tests.py --category state -v
python tests/ha_compat/run_tests.py --category condition -v
```

## Test Categories

**Total: 249 tests across 21 categories (all passing)**

### Core Types (tests/test_core.py)

| Category | Tests | Our Component |
|----------|-------|---------------|
| `state` | 18 | `ha-core::State` |
| `statemachine` | 11 | `ha-state-machine` |
| `eventbus` | 15 | `ha-event-bus` |
| `service` | 10 | `ha-service-registry` |
| `event` | 7 | `ha-core::Event` |
| `context` | 2 | `ha-core::Context` |

### Condition Tests (tests/helpers/test_condition.py)

| Category | Tests | Our Component |
|----------|-------|---------------|
| `condition` | 22 | `ha-automation::Condition` |

### Registry Tests (tests/helpers/)

| Category | Tests | Our Component |
|----------|-------|---------------|
| `storage` | 9 | `ha-registries::Storage` |
| `area_registry` | 17 | `ha-registries::AreaRegistry` |
| `floor_registry` | 14 | `ha-registries::FloorRegistry` |
| `label_registry` | 13 | `ha-registries::LabelRegistry` |
| `entity_registry` | 11 | `ha-registries::EntityRegistry` |
| `device_registry` | 10 | `ha-registries::DeviceRegistry` |

### Template Tests (tests/helpers/template/)

| Category | Tests | Our Component |
|----------|-------|---------------|
| `template` | 24 | `ha-template` |

### Helper Tests (tests/helpers/)

| Category | Tests | Description |
|----------|-------|-------------|
| `helper_state` | 8 | State reproduction helpers |
| `helper_event` | 8 | Event tracking helpers |
| `helper_service` | 8 | Service call helpers |

### API Tests (tests/components/)

| Category | Tests | Our Component |
|----------|-------|---------------|
| `api` | 20 | `ha-api::RestApi` |
| `websocket_commands` | 15 | `ha-api::WebSocketApi` |
| `websocket_messages` | 4 | `ha-api::WebSocketMessages` |
| `websocket_http` | 3 | `ha-api::WebSocketHttp` |

## How It Works

1. **Setup**: Initializes vendored `vendor/ha-core` submodule, installs dependencies
2. **Patch**: `conftest.py` monkey-patches HA's core to use our Rust components
3. **Run**: pytest runs HA's tests with our patched components
4. **Report**: Compare results against baseline (pure Python HA)

## Files

- `setup.sh` - Setup script for HA test environment
- `conftest.py` - Pytest configuration that patches HA core
- `run_tests.py` - Test runner with filtering and reporting
- `run_ha_tests.py` - Simpler runner for individual tests

## Adding New Test Categories

1. Find relevant tests in `vendor/ha-core/tests/`
2. Add category to `TEST_CATEGORIES` in `run_tests.py`
3. Add patching for new components in `conftest.py` if needed
4. Run tests: `python run_tests.py --category <name> -v`
