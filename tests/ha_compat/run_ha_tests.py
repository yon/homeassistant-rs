#!/usr/bin/env python3
"""Run Home Assistant tests with Rust component patching.

This script patches HA's core components with our Rust implementations
before running pytest.

Usage:
    python run_ha_tests.py test_state_init
    python run_ha_tests.py "test_state*"
    python run_ha_tests.py --no-rust test_state_init  # baseline
"""

import argparse
import os
import sys
from pathlib import Path
from unittest.mock import patch

# Paths
REPO_ROOT = Path(__file__).parent.parent.parent
HA_CORE_DIR = REPO_ROOT.parent / "core"
VENV_BIN = REPO_ROOT / ".venv" / "bin"


def setup_rust_patching():
    """Setup the Rust component patching."""
    try:
        import ha_core_rs
    except ImportError:
        print("Warning: ha_core_rs not available, running pure Python")
        return False

    # Import our Rust wrappers - use direct file import
    import importlib.util
    conftest_path = Path(__file__).parent / "conftest.py"
    spec = importlib.util.spec_from_file_location("ha_compat_conftest", conftest_path)
    conftest = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(conftest)
    RustState = conftest.RustState
    RustContext = conftest.RustContext
    RustEvent = conftest.RustEvent
    RustServiceCall = conftest.RustServiceCall

    # Patch homeassistant.core
    import homeassistant.core as ha_core

    # Apply patches
    ha_core.State = RustState
    ha_core.Context = RustContext
    ha_core.Event = RustEvent
    ha_core.ServiceCall = RustServiceCall

    print("=" * 60)
    print("  RUST COMPONENTS PATCHED")
    print("  - homeassistant.core.State -> RustState")
    print("  - homeassistant.core.Context -> RustContext")
    print("  - homeassistant.core.Event -> RustEvent")
    print("  - homeassistant.core.ServiceCall -> RustServiceCall")
    print("=" * 60)

    return True


def main():
    parser = argparse.ArgumentParser(description="Run HA tests with Rust patching")
    parser.add_argument("test_pattern", nargs="?", default="test_state_init",
                        help="Test pattern to run (e.g., 'test_state*')")
    parser.add_argument("--no-rust", action="store_true",
                        help="Run without Rust patching (baseline)")
    parser.add_argument("-v", "--verbose", action="store_true",
                        help="Verbose output")
    args = parser.parse_args()

    # Change to HA core directory
    os.chdir(HA_CORE_DIR)
    sys.path.insert(0, str(HA_CORE_DIR))

    # Setup patching if enabled
    if not args.no_rust:
        rust_enabled = setup_rust_patching()
    else:
        rust_enabled = False
        print("Running in baseline mode (pure Python)")

    # Import pytest after patching
    import pytest

    # Build pytest args
    pytest_args = [
        f"tests/test_core.py",
        "-k", args.test_pattern,
        "--tb=short",
    ]

    if args.verbose:
        pytest_args.append("-v")

    # Run pytest
    print(f"\nRunning: pytest {' '.join(pytest_args)}\n")
    return pytest.main(pytest_args)


if __name__ == "__main__":
    sys.exit(main())
