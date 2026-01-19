"""Helper to load modules from native Home Assistant (vendor/ha-core).

This module provides a way to import from the native HA package without
conflicting with our shim package. The native package is loaded as a
separate namespace.

Usage:
    from homeassistant._native_loader import load_native_module
    _native = load_native_module("homeassistant.const")
    STATE_ON = _native.STATE_ON
"""

from __future__ import annotations

import importlib
import sys
from pathlib import Path
from typing import Any

# Find the vendor/ha-core directory by searching upward from this file
def _find_vendor_path() -> Path:
    """Find vendor/ha-core by searching up from this file's location."""
    # Start from this file and search upward for vendor/ha-core
    current = Path(__file__).resolve().parent
    for _ in range(10):  # Limit search depth
        vendor_path = current / "vendor" / "ha-core"
        if vendor_path.exists():
            return vendor_path
        parent = current.parent
        if parent == current:
            break  # Reached root
        current = parent
    # Fallback: try from cwd
    cwd_vendor = Path.cwd() / "vendor" / "ha-core"
    if cwd_vendor.exists():
        return cwd_vendor
    raise RuntimeError("Could not find vendor/ha-core directory")

_VENDOR_PATH = _find_vendor_path()

# Cache for loaded modules - keyed by module name
_module_cache: dict[str, Any] = {}

# Track if we're currently loading to prevent recursion
_loading: set[str] = set()


def load_native_module(module_name: str) -> Any:
    """Load a module from native Home Assistant.

    Args:
        module_name: Full module path (e.g., "homeassistant.const")

    Returns:
        The loaded module.

    Note:
        This function temporarily modifies sys.path to prioritize vendor/ha-core
        and removes shim modules from sys.modules to ensure native modules are loaded.
    """
    # Return cached module if available
    if module_name in _module_cache:
        return _module_cache[module_name]

    # Prevent recursion
    if module_name in _loading:
        raise ImportError(f"Circular import detected for {module_name}")
    _loading.add(module_name)

    try:
        return _load_native_module_impl(module_name)
    finally:
        _loading.discard(module_name)


def _load_native_module_impl(module_name: str) -> Any:
    """Implementation of native module loading."""
    vendor_str = str(_VENDOR_PATH)

    # Save current sys.path state
    original_path = sys.path.copy()

    # Save and remove ALL homeassistant modules from sys.modules
    # This includes both shim and any native modules that might be there
    saved_modules: dict[str, Any] = {}

    for name in list(sys.modules.keys()):
        if name == "homeassistant" or name.startswith("homeassistant."):
            saved_modules[name] = sys.modules[name]
            del sys.modules[name]

    try:
        # Put vendor/ha-core at the FRONT of sys.path so it's found first
        # Remove any existing shim paths and vendor paths
        new_path = [p for p in sys.path if "python/homeassistant" not in p]
        if vendor_str in new_path:
            new_path.remove(vendor_str)
        new_path.insert(0, vendor_str)
        sys.path = new_path

        # Clear any stale import caches
        importlib.invalidate_caches()

        # Now import the native module
        module = importlib.import_module(module_name)
        _module_cache[module_name] = module

        # Also cache any other native modules that were loaded as dependencies
        # Store them in our cache with a "_native:" prefix so we can find them
        for name in list(sys.modules.keys()):
            if name == "homeassistant" or name.startswith("homeassistant."):
                if name not in _module_cache:
                    _module_cache[name] = sys.modules[name]

        return module
    finally:
        # Restore sys.path
        sys.path = original_path

        # Clear ALL homeassistant modules that were loaded during native import
        for name in list(sys.modules.keys()):
            if name == "homeassistant" or name.startswith("homeassistant."):
                del sys.modules[name]

        # Restore our saved modules (shim modules)
        for name, mod in saved_modules.items():
            sys.modules[name] = mod
