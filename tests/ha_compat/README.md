# Home Assistant Compatibility Tests

This directory contains infrastructure for running Home Assistant's own test suite
against our Rust extension to verify API compatibility.

## Strategy

Rather than reimplementing HA's tests, we run their actual tests with our Rust
components substituted in. This proves compatibility better than any reimplementation.

## Test Categories

We focus on tests for components we've implemented in Rust:

| HA Test File | Our Component | Status |
|--------------|---------------|--------|
| `test_core.py::test_state_*` | `ha-core::State` | Ready |
| `test_core.py::test_statemachine_*` | `ha-state-machine` | Ready |
| `test_core.py::test_eventbus_*` | `ha-event-bus` | Ready |
| `test_core.py::test_service*` | `ha-service-registry` | Ready |

## Quick Start

```bash
# Setup (one-time)
make ha-compat-setup

# Run compatibility tests
make ha-compat-test

# Run specific test category
make ha-compat-test TESTS="test_statemachine"
```

## How It Works

1. **Setup**: Clone HA core, install dependencies, install our wheel
2. **Patch**: Our `conftest.py` monkey-patches HA's core to use Rust components
3. **Run**: pytest runs HA's tests with our patched components
4. **Report**: Compare results against baseline (pure Python HA)

## Files

- `setup.sh` - Setup script for HA test environment
- `conftest.py` - Pytest configuration that patches HA core
- `run_tests.py` - Test runner with filtering and reporting
- `baseline.json` - Expected test results from pure Python HA
