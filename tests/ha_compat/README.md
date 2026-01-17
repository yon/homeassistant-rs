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

### Core Types (tests/test_core.py)

| Category | Our Component | Status |
|----------|---------------|--------|
| `state` | `ha-core::State` | Ready |
| `statemachine` | `ha-state-machine` | Ready |
| `eventbus` | `ha-event-bus` | Ready |
| `service` | `ha-service-registry` | Ready |
| `event` | `ha-core::Event` | Ready |
| `context` | `ha-core::Context` | Ready |

### Helper Modules (tests/helpers/)

| Category | Our Component | Status |
|----------|---------------|--------|
| `condition` | `ha-automation::Condition` | In Progress |
| `trigger` | `ha-automation::Trigger` | In Progress |
| `script` | `ha-script::Action` | In Progress |
| `automation` | `ha-automation::Automation` | In Progress |

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
