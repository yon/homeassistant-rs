"""Tests for shim completeness - ensure shims provide all native HA attributes.

These tests catch issues where partial Rust modules shadow native HA modules,
causing ImportError for constants, classes, or functions that exist in native HA
but weren't exported by the Rust module.
"""

import pytest


class TestDeviceRegistryCompleteness:
    """Test device_registry shim has all expected attributes."""

    def test_has_event_constants(self):
        """Device registry should export event constants."""
        from homeassistant.helpers import device_registry

        assert hasattr(device_registry, "EVENT_DEVICE_REGISTRY_UPDATED")

    def test_has_connection_constants(self):
        """Device registry should export connection type constants."""
        from homeassistant.helpers import device_registry

        assert hasattr(device_registry, "CONNECTION_NETWORK_MAC")
        assert hasattr(device_registry, "CONNECTION_UPNP")
        assert hasattr(device_registry, "CONNECTION_ZIGBEE")

    def test_has_device_entry_type(self):
        """Device registry should export DeviceEntryType enum."""
        from homeassistant.helpers import device_registry

        assert hasattr(device_registry, "DeviceEntryType")

    def test_has_device_info(self):
        """Device registry should export DeviceInfo TypedDict."""
        from homeassistant.helpers import device_registry

        assert hasattr(device_registry, "DeviceInfo")

    def test_has_async_get(self):
        """Device registry should export async_get function."""
        from homeassistant.helpers import device_registry

        assert hasattr(device_registry, "async_get")
        assert callable(device_registry.async_get)


class TestEntityRegistryCompleteness:
    """Test entity_registry shim has all expected attributes."""

    def test_has_event_constants(self):
        """Entity registry should export event constants."""
        from homeassistant.helpers import entity_registry

        assert hasattr(entity_registry, "EVENT_ENTITY_REGISTRY_UPDATED")

    def test_has_disabler_enum(self):
        """Entity registry should export RegistryEntryDisabler enum."""
        from homeassistant.helpers import entity_registry

        assert hasattr(entity_registry, "RegistryEntryDisabler")

    def test_has_hider_enum(self):
        """Entity registry should export RegistryEntryHider enum."""
        from homeassistant.helpers import entity_registry

        assert hasattr(entity_registry, "RegistryEntryHider")

    def test_has_async_get(self):
        """Entity registry should export async_get function."""
        from homeassistant.helpers import entity_registry

        assert hasattr(entity_registry, "async_get")
        assert callable(entity_registry.async_get)


class TestAreaRegistryCompleteness:
    """Test area_registry shim has all expected attributes."""

    def test_has_event_constants(self):
        """Area registry should export event constants."""
        from homeassistant.helpers import area_registry

        assert hasattr(area_registry, "EVENT_AREA_REGISTRY_UPDATED")

    def test_has_async_get(self):
        """Area registry should export async_get function."""
        from homeassistant.helpers import area_registry

        assert hasattr(area_registry, "async_get")
        assert callable(area_registry.async_get)


class TestFloorRegistryCompleteness:
    """Test floor_registry shim has all expected attributes."""

    def test_has_event_constants(self):
        """Floor registry should export event constants."""
        from homeassistant.helpers import floor_registry

        assert hasattr(floor_registry, "EVENT_FLOOR_REGISTRY_UPDATED")

    def test_has_async_get(self):
        """Floor registry should export async_get function."""
        from homeassistant.helpers import floor_registry

        assert hasattr(floor_registry, "async_get")
        assert callable(floor_registry.async_get)


class TestLabelRegistryCompleteness:
    """Test label_registry shim has all expected attributes."""

    def test_has_event_constants(self):
        """Label registry should export event constants."""
        from homeassistant.helpers import label_registry

        assert hasattr(label_registry, "EVENT_LABEL_REGISTRY_UPDATED")

    def test_has_async_get(self):
        """Label registry should export async_get function."""
        from homeassistant.helpers import label_registry

        assert hasattr(label_registry, "async_get")
        assert callable(label_registry.async_get)


class TestStorageCompleteness:
    """Test storage shim has all expected attributes."""

    def test_has_store_class(self):
        """Storage should export Store class."""
        from homeassistant.helpers import storage

        assert hasattr(storage, "Store")

    def test_has_async_migrator(self):
        """Storage should export async_migrator function."""
        from homeassistant.helpers import storage

        assert hasattr(storage, "async_migrator")


class TestTemplateCompleteness:
    """Test template shim has all expected attributes."""

    def test_has_template_class(self):
        """Template should export Template class."""
        from homeassistant.helpers import template

        assert hasattr(template, "Template")

    def test_has_render_complex(self):
        """Template should export render_complex function."""
        from homeassistant.helpers import template

        assert hasattr(template, "render_complex")

    def test_has_template_error(self):
        """Template should export TemplateError exception."""
        from homeassistant.helpers import template

        assert hasattr(template, "TemplateError")


class TestConditionCompleteness:
    """Test condition shim has all expected attributes."""

    def test_has_async_from_config(self):
        """Condition should export async_from_config function."""
        from homeassistant.helpers import condition

        assert hasattr(condition, "async_from_config")

    def test_has_condition_checker_type(self):
        """Condition should export ConditionCheckerType."""
        from homeassistant.helpers import condition

        assert hasattr(condition, "ConditionCheckerType")


class TestTriggerCompleteness:
    """Test trigger shim has all expected attributes."""

    def test_has_async_initialize_triggers(self):
        """Trigger should export async_initialize_triggers function."""
        from homeassistant.helpers import trigger

        assert hasattr(trigger, "async_initialize_triggers")


class TestShimHasFileAttribute:
    """Test that shims are actual Python files, not just Rust modules.

    Rust modules registered in sys.modules don't have __file__.
    This was the root cause of the device_registry import issue.
    """

    @pytest.mark.parametrize(
        "module_path",
        [
            "homeassistant.helpers.device_registry",
            "homeassistant.helpers.entity_registry",
            "homeassistant.helpers.area_registry",
            "homeassistant.helpers.floor_registry",
            "homeassistant.helpers.label_registry",
            "homeassistant.helpers.storage",
            "homeassistant.helpers.template",
            "homeassistant.helpers.condition",
            "homeassistant.helpers.trigger",
        ],
    )
    def test_shim_has_file(self, module_path):
        """Shims should have __file__ attribute (Rust modules don't)."""
        import importlib

        module = importlib.import_module(module_path)

        # This was the bug: Rust modules have no __file__
        assert hasattr(module, "__file__"), f"{module_path} has no __file__ (probably a Rust module shadowing the shim)"
        assert module.__file__ is not None, f"{module_path}.__file__ is None"

    @pytest.mark.parametrize(
        "module_path",
        [
            "homeassistant.helpers.device_registry",
            "homeassistant.helpers.entity_registry",
            "homeassistant.helpers.area_registry",
            "homeassistant.helpers.floor_registry",
            "homeassistant.helpers.label_registry",
            "homeassistant.helpers.storage",
            "homeassistant.helpers.condition",
            "homeassistant.helpers.trigger",
        ],
    )
    def test_shim_from_shim_directory(self, module_path):
        """Shims should be loaded from ha-py-bridge/python directory."""
        import importlib

        module = importlib.import_module(module_path)

        assert "ha-py-bridge/python" in module.__file__, (
            f"{module_path} loaded from wrong location: {module.__file__}"
        )
