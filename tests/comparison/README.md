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
