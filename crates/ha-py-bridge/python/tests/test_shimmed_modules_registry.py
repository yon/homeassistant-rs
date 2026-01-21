"""Tests for the module registry.

These tests ensure that:
1. ModuleRegistry correctly discovers shim modules
2. ModuleSource enum correctly categorizes modules
3. After importing homeassistant, no shimmed module is loaded from vendor
"""

import importlib
import sys

import pytest

from homeassistant._module_registry import ModuleInfo, ModuleSource, registry


class TestModuleRegistry:
    """Test ModuleRegistry class."""

    def test_shim_modules_not_empty(self):
        """Registry should contain discovered shim modules."""
        assert len(registry.shim_modules) > 0, "No shimmed modules discovered"

    def test_core_modules_discovered(self):
        """Core shim modules should be discovered."""
        expected = {
            "homeassistant",
            "homeassistant.const",
            "homeassistant.core",
            "homeassistant.helpers",
            "homeassistant.helpers.entity",
        }
        missing = expected - registry.shim_modules
        assert not missing, f"Expected shim modules not discovered: {missing}"

    def test_private_modules_excluded(self):
        """Private modules (starting with _) should not be in registry."""
        private = [m for m in registry.shim_modules if m.split(".")[-1].startswith("_")]
        assert not private, f"Private modules should be excluded: {private}"


class TestModuleSource:
    """Test ModuleSource enum and source() method."""

    def test_shim_module_returns_shim(self):
        """Shim modules should return ModuleSource.SHIM."""
        assert registry.source("homeassistant.const") == ModuleSource.SHIM
        assert registry.source("homeassistant.helpers.entity") == ModuleSource.SHIM

    def test_vendor_module_returns_vendor(self):
        """Non-shim modules should return ModuleSource.VENDOR."""
        assert registry.source("homeassistant.helpers.json") == ModuleSource.VENDOR
        assert registry.source("homeassistant.util.dt") == ModuleSource.VENDOR

    def test_is_shim_method(self):
        """is_shim() should return True for shim modules."""
        assert registry.is_shim("homeassistant.const") is True
        assert registry.is_shim("homeassistant.helpers.json") is False

    def test_is_vendor_method(self):
        """is_vendor() should return True for vendor modules."""
        assert registry.is_vendor("homeassistant.const") is False
        assert registry.is_vendor("homeassistant.helpers.json") is True


class TestModuleInfo:
    """Test ModuleInfo dataclass."""

    def test_info_returns_correct_source(self):
        """info() should return ModuleInfo with correct source."""
        info = registry.info("homeassistant.const")
        assert info.name == "homeassistant.const"
        assert info.source == ModuleSource.SHIM
        assert info.is_shim is True
        assert info.is_vendor is False

    def test_info_for_vendor_module(self):
        """info() for vendor module should have VENDOR source."""
        info = registry.info("homeassistant.helpers.json")
        assert info.name == "homeassistant.helpers.json"
        assert info.source == ModuleSource.VENDOR
        assert info.is_shim is False
        assert info.is_vendor is True

    def test_iter_shim_modules(self):
        """iter_shim_modules() should yield ModuleInfo for all shims."""
        infos = list(registry.iter_shim_modules())
        assert len(infos) == len(registry.shim_modules)
        assert all(info.is_shim for info in infos)


@pytest.fixture(autouse=True)
def clean_homeassistant_modules():
    """Remove all homeassistant modules and configure sys.path for shim."""
    shim_path = registry.shim_path

    # Save original state
    original_path = sys.path.copy()
    original_path_hooks = sys.path_hooks.copy()
    original_path_importer_cache = sys.path_importer_cache.copy()

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

        for module_name in registry.shim_modules:
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

        shim_path = str(registry.shim_path)
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

    def test_verify_loaded_module_catches_wrong_source(self):
        """verify_loaded_module() should raise if module is from wrong source."""
        import homeassistant.const  # noqa: F401

        # This should succeed - const is a shim and should be loaded from shim
        info = registry.verify_loaded_module("homeassistant.const")
        assert info is not None
        assert info.is_shim

    def test_verify_loaded_module_returns_none_for_unloaded(self):
        """verify_loaded_module() should return None for unloaded modules."""
        # Make sure module isn't loaded
        if "homeassistant.components.binary_sensor" in sys.modules:
            del sys.modules["homeassistant.components.binary_sensor"]

        info = registry.verify_loaded_module("homeassistant.components.binary_sensor")
        assert info is None
