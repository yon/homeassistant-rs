#!/bin/bash
# Setup script for Home Assistant compatibility tests
#
# This script:
# 1. Ensures HA core is cloned
# 2. Installs HA's test dependencies
# 3. Generates translations (required for exception message tests)
# 4. Installs additional test dependencies
# 5. Installs our Rust wheel
# 6. Verifies the setup

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HA_CORE_DIR="$REPO_ROOT/../core"
VENV="$REPO_ROOT/.venv"
HA_VERSION="2026.1.1"

echo "=== Home Assistant Compatibility Test Setup ==="
echo "Repo root: $REPO_ROOT"
echo "HA core dir: $HA_CORE_DIR"
echo "Target HA version: $HA_VERSION"
echo ""

# Step 1: Clone HA core if needed
if [ ! -d "$HA_CORE_DIR" ]; then
    echo "Cloning Home Assistant core ($HA_VERSION)..."
    git clone --depth 1 --branch "$HA_VERSION" \
        https://github.com/home-assistant/core.git "$HA_CORE_DIR"
else
    echo "✓ HA core already exists at $HA_CORE_DIR"
    # Verify version
    cd "$HA_CORE_DIR"
    CURRENT_TAG=$(git describe --tags --exact-match 2>/dev/null || echo "unknown")
    if [ "$CURRENT_TAG" != "$HA_VERSION" ]; then
        echo "  Warning: HA core is at $CURRENT_TAG, expected $HA_VERSION"
        echo "  Run 'rm -rf $HA_CORE_DIR' and re-run setup to update"
    fi
    cd "$REPO_ROOT"
fi

# Step 2: Install HA core with dependencies
echo ""
echo "Installing HA core with dependencies (this may take a while)..."
cd "$HA_CORE_DIR"
"$VENV/bin/pip" install -q -e ".[test]" 2>&1 | tail -10 || {
    echo "Full install failed, trying minimal install..."
    "$VENV/bin/pip" install -q -e . 2>&1 | tail -5
}
cd "$REPO_ROOT"

# Step 3: Generate translations (required for exception message tests)
echo ""
echo "Generating translations..."
cd "$HA_CORE_DIR"
"$VENV/bin/python" -m script.translations develop --all 2>&1 | tail -3
cd "$REPO_ROOT"

# Step 4: Ensure test dependencies are installed
echo ""
echo "Installing additional test dependencies..."
"$VENV/bin/pip" install -q pytest pytest-asyncio pytest-timeout freezegun \
    pytest-socket pytest-xdist respx requests-mock syrupy \
    pytest-unordered 2>&1 | tail -3

# Step 5: Build and install our Rust wheel
echo ""
echo "Building Rust wheel..."
"$VENV/bin/maturin" develop -q

# Step 6: Verify setup
echo ""
echo "Verifying setup..."
"$VENV/bin/python" -c "
import sys
print('Python:', sys.version.split()[0])

# Check HA core
import homeassistant.core as ha
print('HA core:', ha.__file__)

# Check our extension
import ha_core_rs
print('ha_core_rs: loaded')

# Quick smoke test
from ha_core_rs import HomeAssistant, StateMachine, EventBus, ServiceRegistry
hass = HomeAssistant()
hass.states.set('test.entity', 'on', {})
assert hass.states.get('test.entity').state == 'on'
print('Smoke test: passed')
"

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Run compatibility tests with:"
echo "  make ha-compat-test"
echo ""
echo "Or run specific tests:"
echo "  $VENV/bin/pytest $HA_CORE_DIR/tests/test_core.py::test_state_init -v"
