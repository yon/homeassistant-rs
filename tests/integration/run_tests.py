#!/usr/bin/env python3
"""Run tests against the Rust HA server.

This script runs pytest tests that verify our Rust WebSocket and REST API
implementations work correctly.

Usage:
    python run_tests.py                     # Run all tests
    python run_tests.py -v                  # Verbose output
    python run_tests.py -k "device"         # Run only device tests
    python run_tests.py --build             # Build server before testing
"""

import argparse
import os
import subprocess
import sys
from pathlib import Path


def get_repo_root() -> Path:
    """Get the repository root directory."""
    return Path(__file__).parent.parent.parent


def build_server() -> int:
    """Build the Rust server."""
    print("Building Rust server...")
    result = subprocess.run(
        ["cargo", "build", "-p", "ha-server"],
        cwd=get_repo_root(),
    )
    return result.returncode


def run_tests(pytest_args: list[str]) -> int:
    """Run the pytest tests."""
    repo_root = get_repo_root()
    test_dir = repo_root / "tests" / "rust_server"
    venv_pytest = repo_root / ".venv" / "bin" / "pytest"

    if venv_pytest.exists():
        pytest_cmd = str(venv_pytest)
    else:
        pytest_cmd = "pytest"

    cmd = [
        pytest_cmd,
        str(test_dir),
        "--tb=short",
        "-x",  # Stop on first failure
    ] + pytest_args

    print(f"Running: {' '.join(cmd)}")
    result = subprocess.run(cmd, cwd=repo_root)
    return result.returncode


def main() -> int:
    parser = argparse.ArgumentParser(description="Run Rust server tests")
    parser.add_argument("--build", "-b", action="store_true",
                        help="Build the server before testing")
    parser.add_argument("-v", "--verbose", action="store_true",
                        help="Verbose pytest output")
    parser.add_argument("-k", "--filter", type=str,
                        help="Filter tests by keyword expression")
    parser.add_argument("pytest_args", nargs="*",
                        help="Additional pytest arguments")

    args = parser.parse_args()

    if args.build:
        ret = build_server()
        if ret != 0:
            print("Build failed!")
            return ret

    pytest_args = args.pytest_args or []
    if args.verbose:
        pytest_args.append("-v")
    if args.filter:
        pytest_args.extend(["-k", args.filter])

    return run_tests(pytest_args)


if __name__ == "__main__":
    sys.exit(main())
