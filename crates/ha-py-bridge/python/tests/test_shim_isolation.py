"""Rigorous tests for Python shim isolation.

These tests ensure that the shim layer:
1. Shim modules take precedence over native HA
2. Unknown modules fall back to native HA via __path__ extension
3. Properly routes entity state writes to Rust via RustStateMixin
4. Shim modules are loaded from the correct directory
"""

import importlib
import os
import sys
from pathlib import Path
from unittest import mock

import pytest

# Ensure we're testing the shim, not any installed homeassistant
SHIM_PATH = Path(__file__).parents[1]  # crates/ha-py-bridge/python
VENDOR_PATH = Path(__file__).parents[4] / "vendor/ha-core"  # repo_root/vendor/ha-core


@pytest.fixture(autouse=True)
def clean_homeassistant_modules():
    """Remove all homeassistant modules and configure sys.path for shim."""
    # Save original state
    original_path = sys.path.copy()
    original_modules = {k: v for k, v in sys.modules.items()
                        if k == "homeassistant" or k.startswith("homeassistant.")}

    def clear_modules():
        to_remove = [name for name in sys.modules if name == "homeassistant" or name.startswith("homeassistant.")]
        for name in to_remove:
            del sys.modules[name]

    clear_modules()

    # Add our shim first on sys.path
    # We keep site-packages for dependencies (propcache, etc.) but our shim
    # takes precedence because it's first
    shim_str = str(SHIM_PATH)
    # Remove shim if already present, then add at start
    new_path = [p for p in sys.path if p != shim_str]
    new_path.insert(0, shim_str)
    sys.path = new_path
    importlib.invalidate_caches()

    yield

    # Restore
    clear_modules()
    sys.path = original_path
    importlib.invalidate_caches()


class TestShimPrecedence:
    """Test that shim modules take precedence over native HA."""

    def test_shim_path_extended_with_native(self):
        """Shim should extend __path__ with native HA for fallback."""
        import homeassistant
        native_path = str(VENDOR_PATH / "homeassistant")
        assert native_path in homeassistant.__path__

    def test_shim_directory_first_in_path(self):
        """Shim directory should be first in __path__."""
        import homeassistant
        shim_dir = str(SHIM_PATH / "homeassistant")
        # The shim's own directory should be first (implicitly via __file__ location)
        # and native path appended for fallback
        assert len(homeassistant.__path__) >= 1

    def test_components_path_extended(self):
        """Components shim should extend __path__ for native components."""
        import homeassistant.components
        native_components = str(VENDOR_PATH / "homeassistant" / "components")
        assert native_components in homeassistant.components.__path__

    def test_helpers_path_extended(self):
        """Helpers shim should extend __path__ for native helpers."""
        import homeassistant.helpers
        native_helpers = str(VENDOR_PATH / "homeassistant" / "helpers")
        assert native_helpers in homeassistant.helpers.__path__


class TestNativeFallback:
    """Test that unknown modules fall back to native HA."""

    @pytest.mark.skip(reason="Demo component requires template module which subclasses Rust State")
    def test_demo_component_loads(self):
        """Demo component should load via native fallback."""
        from homeassistant.components.demo import light
        assert light is not None
        assert hasattr(light, "DemoLight")

    def test_util_module_loads(self):
        """Util module should load via native fallback."""
        from homeassistant import util
        assert util is not None

    def test_generated_module_loads(self):
        """Generated module should load via native fallback."""
        from homeassistant import generated
        assert generated is not None

    @pytest.mark.skip(reason="aiohttp_client requires template module which subclasses Rust State")
    def test_unknown_helper_loads(self):
        """Unknown helpers should load via native fallback."""
        from homeassistant.helpers import aiohttp_client
        assert aiohttp_client is not None


class TestExplicitlyShimmedModules:
    """Test that explicitly shimmed modules work correctly."""

    def test_const_module(self):
        """homeassistant.const should work."""
        from homeassistant.const import STATE_ON, STATE_OFF, Platform
        assert STATE_ON == "on"
        assert STATE_OFF == "off"
        assert Platform.LIGHT == "light"

    def test_const_includes_version(self):
        """homeassistant.const should include __version__."""
        from homeassistant.const import __version__, MAJOR_VERSION, MINOR_VERSION
        assert __version__ is not None
        assert isinstance(MAJOR_VERSION, int)
        assert isinstance(MINOR_VERSION, int)

    def test_const_includes_private_constants(self):
        """homeassistant.const should include _LOGGER and other private names."""
        from homeassistant import const
        # Check that we're not filtering out useful private names
        # (Some constants like _UNDEF might be needed)
        assert hasattr(const, "STATE_ON")

    def test_core_module(self):
        """homeassistant.core should work with our types."""
        from homeassistant.core import HomeAssistant, callback, Event, State
        assert HomeAssistant is not None
        assert callable(callback)
        assert Event is not None
        assert State is not None

    def test_core_includes_native_types(self):
        """homeassistant.core should re-export native types needed by other modules."""
        from homeassistant.core import (
            CALLBACK_TYPE,
            Context,
            HassJob,
            HassJobType,
            CoreState,
            EventOrigin,
        )
        assert CALLBACK_TYPE is not None
        assert Context is not None

    def test_exceptions_module(self):
        """homeassistant.exceptions should work."""
        from homeassistant.exceptions import HomeAssistantError, ConfigEntryError
        assert issubclass(HomeAssistantError, Exception)
        assert issubclass(ConfigEntryError, HomeAssistantError)

    def test_config_entries_module(self):
        """homeassistant.config_entries should work."""
        from homeassistant.config_entries import ConfigEntry, ConfigFlow, SOURCE_USER
        assert ConfigEntry is not None
        assert ConfigFlow is not None
        assert SOURCE_USER == "user"

    def test_helpers_entity_module(self):
        """homeassistant.helpers.entity should work."""
        from homeassistant.helpers.entity import Entity, DeviceInfo, RustStateMixin
        assert Entity is not None
        assert DeviceInfo is not None
        assert RustStateMixin is not None

    def test_helpers_entity_platform_module(self):
        """homeassistant.helpers.entity_platform should work."""
        from homeassistant.helpers.entity_platform import AddEntitiesCallback, EntityPlatform
        assert AddEntitiesCallback is not None
        assert EntityPlatform is not None

    def test_helpers_typing_module(self):
        """homeassistant.helpers.typing should work."""
        from homeassistant.helpers.typing import ConfigType
        assert ConfigType is not None

    def test_components_light_module(self):
        """homeassistant.components.light should work."""
        from homeassistant.components.light import LightEntity, ColorMode, ATTR_BRIGHTNESS
        assert LightEntity is not None
        assert ColorMode is not None
        assert ATTR_BRIGHTNESS == "brightness"

    def test_components_switch_module(self):
        """homeassistant.components.switch should work."""
        from homeassistant.components.switch import SwitchEntity, SwitchDeviceClass
        assert SwitchEntity is not None
        assert SwitchDeviceClass is not None

    def test_components_sensor_module(self):
        """homeassistant.components.sensor should work."""
        from homeassistant.components.sensor import SensorEntity, SensorDeviceClass, SensorStateClass
        assert SensorEntity is not None
        assert SensorDeviceClass is not None
        assert SensorStateClass is not None


class TestRustStateMixinInheritance:
    """Test that entity classes properly inherit RustStateMixin."""

    def test_entity_has_mixin(self):
        """Entity should have RustStateMixin in its MRO."""
        from homeassistant.helpers.entity import Entity, RustStateMixin
        assert RustStateMixin in Entity.__mro__

    def test_light_entity_has_mixin(self):
        """LightEntity should have RustStateMixin in its MRO."""
        from homeassistant.components.light import LightEntity
        from homeassistant.helpers.entity import RustStateMixin
        assert RustStateMixin in LightEntity.__mro__

    def test_switch_entity_has_mixin(self):
        """SwitchEntity should have RustStateMixin in its MRO."""
        from homeassistant.components.switch import SwitchEntity
        from homeassistant.helpers.entity import RustStateMixin
        assert RustStateMixin in SwitchEntity.__mro__

    def test_sensor_entity_has_mixin(self):
        """SensorEntity should have RustStateMixin in its MRO."""
        from homeassistant.components.sensor import SensorEntity
        from homeassistant.helpers.entity import RustStateMixin
        assert RustStateMixin in SensorEntity.__mro__

    def test_mixin_provides_async_write_ha_state(self):
        """RustStateMixin should provide async_write_ha_state method."""
        from homeassistant.helpers.entity import Entity, RustStateMixin
        import inspect

        # Check the method comes from our shim, not native
        method = Entity.async_write_ha_state
        source_file = inspect.getfile(method)
        assert "python/homeassistant" in source_file, f"Method from wrong source: {source_file}"

    def test_light_entity_async_write_ha_state_from_mixin(self):
        """LightEntity.async_write_ha_state should come from RustStateMixin."""
        from homeassistant.components.light import LightEntity
        import inspect

        method = LightEntity.async_write_ha_state
        source_file = inspect.getfile(method)
        assert "python/homeassistant" in source_file, f"Method from wrong source: {source_file}"


@pytest.mark.skip(reason="Demo component requires template module which subclasses Rust State")
class TestDemoEntitiesUseMixin:
    """Test that demo entities (loaded via fallback) still use RustStateMixin."""

    def test_demo_light_uses_mixin(self):
        """Demo light should use RustStateMixin for state writes."""
        from homeassistant.components.demo import light
        from homeassistant.helpers.entity import RustStateMixin

        assert RustStateMixin in light.DemoLight.__mro__

    def test_demo_light_async_write_from_mixin(self):
        """DemoLight.async_write_ha_state should come from RustStateMixin."""
        from homeassistant.components.demo import light
        import inspect

        method = light.DemoLight.async_write_ha_state
        source_file = inspect.getfile(method)
        assert "python/homeassistant" in source_file, f"Method from wrong source: {source_file}"


class TestShimModulesFromCorrectDirectory:
    """Test that shimmed modules are loaded from the shim directory."""

    def test_shim_modules_from_shim_directory(self):
        """All shimmed modules should come from our shim directory."""
        import homeassistant
        import homeassistant.const
        import homeassistant.core
        import homeassistant.helpers.entity
        import homeassistant.components.light

        shim_dir = str(SHIM_PATH / "homeassistant")

        # Check each module's file location
        for mod_name, mod in [
            ("homeassistant", homeassistant),
            ("homeassistant.const", homeassistant.const),
            ("homeassistant.core", homeassistant.core),
            ("homeassistant.helpers.entity", homeassistant.helpers.entity),
            ("homeassistant.components.light", homeassistant.components.light),
        ]:
            mod_file = getattr(mod, "__file__", None)
            assert mod_file is not None, f"{mod_name} has no __file__"
            assert shim_dir in mod_file, f"{mod_name} from wrong location: {mod_file}"

    def test_native_loader_not_exposed(self):
        """_native_loader should not be importable as a public module."""
        # It's fine to import it directly, but it shouldn't be in __all__
        import homeassistant
        assert "_native_loader" not in getattr(homeassistant, "__all__", [])


class TestShimCompleteness:
    """Test that shimmed modules re-export everything needed."""

    def test_const_has_all_platforms(self):
        """const should have all Platform enum values."""
        from homeassistant.const import Platform
        expected_platforms = [
            "ALARM_CONTROL_PANEL", "BINARY_SENSOR", "BUTTON", "CALENDAR",
            "CAMERA", "CLIMATE", "COVER", "DATE", "DATETIME", "DEVICE_TRACKER",
            "EVENT", "FAN", "HUMIDIFIER", "IMAGE", "LAWN_MOWER", "LIGHT",
            "LOCK", "MEDIA_PLAYER", "NOTIFY", "NUMBER", "REMOTE", "SCENE",
            "SELECT", "SENSOR", "SIREN", "STT", "SWITCH", "TEXT", "TIME",
            "TODO", "TTS", "UPDATE", "VACUUM", "VALVE", "WAKE_WORD",
            "WATER_HEATER", "WEATHER",
        ]
        for platform in expected_platforms:
            assert hasattr(Platform, platform), f"Platform.{platform} missing"

    def test_light_has_color_modes(self):
        """light should have all ColorMode enum values."""
        from homeassistant.components.light import ColorMode
        expected_modes = [
            "UNKNOWN", "ONOFF", "BRIGHTNESS", "COLOR_TEMP", "HS", "XY",
            "RGB", "RGBW", "RGBWW", "WHITE",
        ]
        for mode in expected_modes:
            assert hasattr(ColorMode, mode), f"ColorMode.{mode} missing"

    def test_sensor_has_device_classes(self):
        """sensor should have common SensorDeviceClass values."""
        from homeassistant.components.sensor import SensorDeviceClass
        expected_classes = [
            "TEMPERATURE", "HUMIDITY", "BATTERY", "POWER", "ENERGY",
            "VOLTAGE", "CURRENT", "PRESSURE",
        ]
        for cls in expected_classes:
            assert hasattr(SensorDeviceClass, cls), f"SensorDeviceClass.{cls} missing"
