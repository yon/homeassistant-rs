# Home Assistant API Comparison Tests

This directory contains infrastructure for testing our Rust HA implementation
against a real Python Home Assistant instance to ensure API compatibility.

## Architecture

```
┌─────────────────────┐      ┌─────────────────────┐
│  Python HA          │      │  Rust HA            │
│  (Docker)           │      │  (ha-server)        │
│  localhost:18123    │      │  localhost:18124    │
└──────────┬──────────┘      └──────────┬──────────┘
           │                            │
           └────────────┬───────────────┘
                        │
                 ┌──────▼──────┐
                 │  Comparison │
                 │  Test Suite │
                 │  (Rust)     │
                 └─────────────┘
```

## Quick Start

### Local Testing

```bash
# 1. Setup and start the Python HA test instance
make ha-start

# 2. Start the Rust HA server (in another terminal)
HA_PORT=18124 make run

# 3. Run comparison tests
make test-compare

# 4. Stop HA when done
make ha-stop
```

### CI Testing

Comparison tests run automatically:
- On every push to `main`
- On PRs with the `run-comparison` label
- Nightly against multiple HA versions

## Files

| File | Purpose |
|------|---------|
| `ha-versions.toml` | Tracks HA versions we test against |
| `docker-compose.yml` | Docker setup for HA test instance |
| `setup-ha-test.sh` | Generates HA config with test credentials |
| `ha-config/` | Generated HA configuration (gitignored) |
| `mod.rs` | Rust module exports |
| `config.rs` | Test configuration loading |
| `client.rs` | HTTP client for API calls |
| `compare.rs` | Response comparison utilities |
| `harness.rs` | Test harness and suites |

## Test Credentials

The setup script creates a test instance with:
- **Username**: `admin`
- **Password**: `test-password-123`
- **API Token**: `test_api_token_for_comparison_testing_do_not_use_in_production`

⚠️ **DO NOT use these credentials in production!**

## Version Tracking

Edit `ha-versions.toml` to update tested versions:

```toml
[primary]
version = "2026.1.1"
docker_image = "ghcr.io/home-assistant/home-assistant:2026.1.1"
release_date = "2026-01-07"
```

When a new HA version is released:
1. Update `[primary]` to the new version
2. Move the old primary to `[previous]`
3. Run comparison tests to check for regressions
4. Fix any API incompatibilities

## Keeping Up-to-Date Workflow

The comparison tests are the **source of truth** for API compatibility. They compare
live responses from Python HA against our Rust implementation, eliminating the need
for manually maintained schemas.

### When HA Updates Their API

| Scenario | What Happens | Action Required |
|----------|--------------|-----------------|
| New HA version released | Update `ha-versions.toml`, run `make test-compare` | Fix any failures |
| Response format changes | Tests fail with `VALUE` or `STRUCTURE` diffs | Update Rust implementation |
| New fields added | Tests show `MISSING` (field in Python, not in Rust) | Add field to Rust response |
| Fields removed | Tests show `EXTRA` (field in Rust, not in Python) | Remove field from Rust |
| Field types change | Tests show `VALUE` differences | Update Rust field types |

### Comparison Output

```
--- Registry WebSocket API ---
✅ ws:device_registry_list - PASS
✅ ws:entity_registry_list - PASS
❌ ws:area_registry_list - FAIL (2 differences)
   [  MISSING] result[0].new_field : Python="value" Rust="(missing)"
   [    VALUE] result[0].old_field : Python="new_format" Rust="old_format"
```

### CI Integration

For automated compatibility checking, add to `.github/workflows/`:

```yaml
name: HA Compatibility Check
on:
  schedule:
    - cron: '0 0 * * *'  # Nightly
  push:
    branches: [main]

jobs:
  compare:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Start Python HA
        run: make ha-start
      - name: Build Rust HA
        run: cargo build -p ha-server
      - name: Start Rust HA
        run: HA_PORT=18124 ./target/debug/homeassistant &
      - name: Run comparison tests
        run: make test-compare
      - name: Stop servers
        run: make ha-stop
```

### Running Specific Comparison Tests

```bash
# All comparisons
make test-compare

# Just registry endpoints (device, entity, area, floor, label, config_entries)
PYTHON_HA_URL=http://localhost:18123 RUST_HA_URL=http://localhost:18124 \
  cargo test -p ha-test-comparison test_registry_endpoints -- --ignored --nocapture

# Just basic endpoints
PYTHON_HA_URL=http://localhost:18123 RUST_HA_URL=http://localhost:18124 \
  cargo test -p ha-test-comparison test_basic_endpoints -- --ignored --nocapture
```

## Makefile Targets

| Target | Description |
|--------|-------------|
| `make ha-start` | Start HA test instance |
| `make ha-stop` | Stop HA test instance |
| `make ha-logs` | View HA container logs |
| `make ha-status` | Check HA status |
| `make ha-shell` | Open shell in HA container |
| `make ha-version` | Show configured HA versions |
| `make test-compare` | Run comparison tests |
| `make test-compare-ci` | Full CI comparison (start HA, test, stop) |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PYTHON_HA_URL` | `http://localhost:18123` | Python HA instance URL |
| `RUST_HA_URL` | `http://localhost:18124` | Rust HA instance URL |
| `PYTHON_HA_TOKEN` | (from setup script) | API token for Python HA |
| `HA_VERSION` | `2026.1.1` | HA version being tested |

## Adding New Comparison Tests

1. Add the test to `harness.rs` in the appropriate `TestSuites` method
2. Handle any fields that should be ignored (timestamps, IDs) in `CompareOptions`
3. Run locally to verify
4. The test will automatically run in CI

Example:

```rust
// In harness.rs
pub async fn run_my_endpoints(harness: &mut TestHarness) {
    // Test a GET endpoint
    harness
        .compare_get("/api/my_endpoint", None)
        .await
        .print_summary();

    // Test a POST endpoint
    harness
        .compare_post(
            "/api/my_endpoint",
            Some(json!({"key": "value"})),
            None,
        )
        .await
        .print_summary();
}
```

## Troubleshooting

### HA won't start
```bash
# Check logs
make ha-logs

# Reset configuration
rm -rf tests/comparison/ha-config
make ha-setup
make ha-start
```

### Authentication failures
```bash
# Verify token file exists
cat tests/comparison/ha-config/test-token.txt

# Test manually
curl -H "Authorization: Bearer $(cat tests/comparison/ha-config/test-token.txt)" \
  http://localhost:18123/api/
```

### Tests hang waiting for servers
```bash
# Check both servers are running
curl http://localhost:18123/api/  # Python HA
curl http://localhost:18124/api/  # Rust HA
```
