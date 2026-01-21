"""Tests for the SHIMMED_MODULES registry.

These tests ensure that:
1. SHIMMED_MODULES is auto-discovered from shim files on disk
2. After importing homeassistant, no shimmed module is loaded from vendor
"""

import importlib
import sys

import pytest

from homeassistant._native_loader import (
    SHIMMED_MODULES,
    get_shim_path,
)


class TestShimmedModulesDiscovery:
    """Test that SHIMMED_MODULES is correctly auto-discovered."""

    def test_shimmed_modules_not_empty(self):
        """SHIMMED_MODULES should contain discovered shim modules."""
        assert len(SHIMMED_MODULES) > 0, "No shimmed modules discovered"

    def test_core_modules_discovered(self):
        """Core shim modules should be discovered."""
        expected = {
            "homeassistant",
            "homeassistant.const",
            "homeassistant.core",
            "homeassistant.helpers",
            "homeassistant.helpers.entity",
        }
        missing = expected - SHIMMED_MODULES
        assert not missing, f"Expected shim modules not discovered: {missing}"

    def test_private_modules_excluded(self):
        """Private modules (starting with _) should not be in SHIMMED_MODULES."""
        private_modules = [m for m in SHIMMED_MODULES if m.split(".")[-1].startswith("_")]
        assert not private_modules, f"Private modules should be excluded: {private_modules}"


@pytest.fixture(autouse=True)
def clean_homeassistant_modules():
    """Remove all homeassistant modules and configure sys.path for shim."""
    shim_path = get_shim_path()

    # Save original state
    original_path = sys.path.copy()
    original_path_hooks = sys.path_hooks.copy()
    original_path_importer_cache = sys.path_importer_cache.copy()
    original_modules = {
        k: v
        for k, v in sys.modules.items()
        if k == "homeassistant" or k.startswith("homeassistant.")
    }

    def clear_modules():
        to_remove = [
            name
            for name in sys.modules
            if name == "homeassistant" or name.startswith("homeassistant.")
        ]
        for name in to_remove:
            del sys.modules[name]

    clear_modules()

    # Remove editable install path hooks
    sys.path_hooks = [
        hook
        for hook in sys.path_hooks
        if not (
            hasattr(hook, "__self__")
            and "editable" in getattr(hook.__self__, "__module__", "").lower()
        )
    ]

    # Remove editable path entries and add shim first
    shim_str = str(shim_path)
    new_path = [p for p in sys.path if p != shim_str and "__editable__" not in p]
    new_path.insert(0, shim_str)
    sys.path = new_path

    # Clear caches
    sys.path_importer_cache.clear()
    importlib.invalidate_caches()

    yield

    # Restore
    clear_modules()
    sys.path = original_path
    sys.path_hooks = original_path_hooks
    sys.path_importer_cache.clear()
    sys.path_importer_cache.update(original_path_importer_cache)
    importlib.invalidate_caches()


class TestNoShimmedModuleFromVendor:
    """Test that shimmed modules are never loaded from vendor."""

    def test_shimmed_modules_not_from_vendor_after_import(self):
        """After importing homeassistant, no shimmed module should be from vendor."""
        import homeassistant  # noqa: F401

        vendor_loaded = []

        for module_name in SHIMMED_MODULES:
            if module_name in sys.modules:
                mod = sys.modules[module_name]
                origin = getattr(mod, "__file__", None)
                if origin and "vendor/ha-core" in origin:
                    vendor_loaded.append(f"{module_name}: {origin}")

        assert not vendor_loaded, (
            f"These shimmed modules were loaded from vendor instead of shim:\n"
            f"  {chr(10).join(vendor_loaded)}\n"
            f"This indicates a bug in _native_loader.py"
        )

    def test_shimmed_modules_from_shim_after_explicit_import(self):
        """After explicitly importing shimmed modules, they should be from shim."""
        import homeassistant  # noqa: F401

        shim_path = str(get_shim_path())
        wrong_source = []

        # Import a sampling of shimmed modules explicitly
        test_modules = [
            "homeassistant.const",
            "homeassistant.core",
            "homeassistant.helpers",
            "homeassistant.helpers.entity",
            "homeassistant.components.light",
        ]

        for module_name in test_modules:
            importlib.import_module(module_name)
            mod = sys.modules[module_name]
            origin = getattr(mod, "__file__", None)

            if origin and shim_path not in origin:
                wrong_source.append(f"{module_name}: {origin}")

        assert not wrong_source, (
            f"These shimmed modules were not loaded from shim:\n"
            f"  {chr(10).join(wrong_source)}"
        )
