#!/usr/bin/env python3
"""Run Home Assistant compatibility tests.

This script runs HA's own tests for components we've implemented in Rust,
with our Rust extension monkey-patched in place of Python implementations.

Usage:
    python run_tests.py                    # Run all compatible tests
    python run_tests.py --category state   # Run only state-related tests
    python run_tests.py --list             # List available test categories
    python run_tests.py --baseline         # Generate baseline from pure Python HA
"""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

# Test categories mapped to pytest patterns
TEST_CATEGORIES = {
    "state": [
        "test_core.py::test_state_init",
        "test_core.py::test_state_domain",
        "test_core.py::test_state_object_id",
        "test_core.py::test_state_name_if_no_friendly_name_attr",
        "test_core.py::test_state_name_if_friendly_name_attr",
        "test_core.py::test_state_dict_conversion",
        "test_core.py::test_state_repr",
        "test_core.py::test_state_as_dict",
        "test_core.py::test_state_timestamps",
    ],
    "statemachine": [
        "test_core.py::test_statemachine_is_state",
        "test_core.py::test_statemachine_entity_ids",
        "test_core.py::test_statemachine_remove",
        "test_core.py::test_state_machine_case_insensitivity",
        "test_core.py::test_statemachine_last_changed_not_updated_on_same_state",
        "test_core.py::test_statemachine_force_update",
    ],
    "eventbus": [
        "test_core.py::test_eventbus_add_remove_listener",
        "test_core.py::test_eventbus_filtered_listener",
        "test_core.py::test_eventbus_unsubscribe_listener",
        "test_core.py::test_eventbus_listen_once_event_with_callback",
        "test_core.py::test_eventbus_listen_once_event_with_coroutine",
    ],
    "service": [
        "test_core.py::test_service_call_repr",
        "test_core.py::test_service_registry_has_service",
        "test_core.py::test_service_registry_service_enumeration",
        "test_core.py::test_serviceregistry_remove_service",
    ],
    "event": [
        "test_core.py::test_event_eq",
        "test_core.py::test_event_time",
        "test_core.py::test_event_repr",
        "test_core.py::test_event_as_dict",
    ],
    "context": [
        "test_core.py::test_context",
        "test_core.py::test_context_json_fragment",
    ],
}

def get_repo_root() -> Path:
    """Get the repository root directory."""
    return Path(__file__).parent.parent.parent

def get_ha_core_dir() -> Path:
    """Get the HA core directory."""
    return get_repo_root().parent / "core"

def list_categories():
    """List available test categories."""
    print("Available test categories:")
    print("")
    for category, tests in TEST_CATEGORIES.items():
        print(f"  {category}:")
        for test in tests[:3]:
            print(f"    - {test}")
        if len(tests) > 3:
            print(f"    ... and {len(tests) - 3} more")
        print("")

def run_tests(categories: list[str] | None = None, verbose: bool = False,
              use_rust: bool = True) -> int:
    """Run the compatibility tests.

    Args:
        categories: List of test categories to run, or None for all
        verbose: Enable verbose output
        use_rust: If True, patch in Rust components; if False, run pure Python

    Returns:
        Exit code (0 for success)
    """
    repo_root = get_repo_root()
    ha_core = get_ha_core_dir()
    venv = repo_root / ".venv"

    if not ha_core.exists():
        print(f"Error: HA core not found at {ha_core}")
        print("Run: ./tests/ha_compat/setup.sh")
        return 1

    # Build test patterns
    if categories:
        patterns = []
        for cat in categories:
            if cat in TEST_CATEGORIES:
                patterns.extend(TEST_CATEGORIES[cat])
            else:
                print(f"Warning: Unknown category '{cat}'")
        if not patterns:
            print("No valid test patterns found")
            return 1
    else:
        # All categories
        patterns = []
        for tests in TEST_CATEGORIES.values():
            patterns.extend(tests)

    # Build pytest command
    pytest_args = [
        str(venv / "bin" / "pytest"),
        "-v" if verbose else "-q",
        "--tb=short",
        "-x",  # Stop on first failure
    ]

    # Add conftest path for Rust patching
    # We copy our conftest to HA's test directory so pytest auto-discovers it
    rust_conftest_path = None
    if use_rust:
        import shutil
        our_conftest = Path(__file__).parent / "conftest.py"
        # Use a unique name so it doesn't conflict with HA's conftest.py
        rust_conftest_path = ha_core / "conftest_rust.py"
        shutil.copy(our_conftest, rust_conftest_path)

    # Add test patterns (relative to ha_core since we run from there)
    for pattern in patterns:
        pytest_args.append(f"tests/{pattern}")

    print(f"Running {len(patterns)} tests...")
    if use_rust:
        print("Mode: Rust extension patched in")
    else:
        print("Mode: Pure Python (baseline)")
    print("")

    # Run pytest with PYTHONPATH set to include repo root for our conftest import
    env = os.environ.copy()
    pythonpath_parts = [str(repo_root)]
    if "PYTHONPATH" in env:
        pythonpath_parts.append(env["PYTHONPATH"])
    env["PYTHONPATH"] = os.pathsep.join(pythonpath_parts)

    # Run from HA core directory so HA's tests can find their modules
    try:
        result = subprocess.run(pytest_args, cwd=ha_core, env=env)
        return result.returncode
    finally:
        # Clean up the temporary conftest
        if rust_conftest_path and rust_conftest_path.exists():
            rust_conftest_path.unlink()

def main():
    parser = argparse.ArgumentParser(description="Run HA compatibility tests")
    parser.add_argument("--list", action="store_true", help="List test categories")
    parser.add_argument("--category", "-c", action="append",
                        help="Test category to run (can specify multiple)")
    parser.add_argument("--verbose", "-v", action="store_true", help="Verbose output")
    parser.add_argument("--baseline", action="store_true",
                        help="Run without Rust patches (pure Python)")
    parser.add_argument("--all", "-a", action="store_true", help="Run all categories")

    args = parser.parse_args()

    if args.list:
        list_categories()
        return 0

    categories = args.category if args.category else None
    if args.all:
        categories = None

    return run_tests(
        categories=categories,
        verbose=args.verbose,
        use_rust=not args.baseline
    )

if __name__ == "__main__":
    sys.exit(main())
