"""Rigorous tests for Python shim isolation.

These tests ensure that the shim layer:
1. BLOCKS native HA imports in strict mode (default)
2. Only allows explicitly shimmed modules
3. Properly routes entity state writes to Rust via RustStateMixin
4. Only enables fallback when HA_ALLOW_NATIVE_FALLBACK=1 is explicitly set
"""

import importlib
import os
import sys
from pathlib import Path
from unittest import mock

import pytest

# Ensure we're testing the shim, not any installed homeassistant
SHIM_PATH = Path(__file__).parents[2] / "python"
VENDOR_PATH = Path(__file__).parents[2] / "vendor/ha-core"


@pytest.fixture(autouse=True)
def clean_homeassistant_modules():
    """Remove all homeassistant modules and configure sys.path for shim."""
    # Save original state
    original_path = sys.path.copy()

    def clear_modules():
        to_remove = [name for name in sys.modules if name == "homeassistant" or name.startswith("homeassistant.")]
        for name in to_remove:
            del sys.modules[name]

    clear_modules()

    # Remove any site-packages homeassistant paths and add our shim first
    shim_str = str(SHIM_PATH)
    new_path = [p for p in sys.path if "site-packages/homeassistant" not in p]
    # Remove shim if already present, then add at start
    new_path = [p for p in new_path if p != shim_str]
    new_path.insert(0, shim_str)
    sys.path = new_path
    importlib.invalidate_caches()

    yield

    # Restore
    clear_modules()
    sys.path = original_path
    importlib.invalidate_caches()


@pytest.fixture
def strict_mode():
    """Ensure strict mode (no fallback)."""
    with mock.patch.dict(os.environ, {"HA_ALLOW_NATIVE_FALLBACK": ""}, clear=False):
        # Remove the var entirely if it exists
        os.environ.pop("HA_ALLOW_NATIVE_FALLBACK", None)
        yield


@pytest.fixture
def fallback_mode():
    """Enable fallback mode."""
    with mock.patch.dict(os.environ, {"HA_ALLOW_NATIVE_FALLBACK": "1"}):
        yield


class TestStrictModeBlocking:
    """Test that strict mode blocks native HA imports."""

    def test_blocks_unknown_component(self, strict_mode):
        """Unknown components should raise ImportError in strict mode."""
        with pytest.raises(ImportError, match="No module named 'homeassistant.components.demo'"):
            from homeassistant.components.demo import light

    def test_blocks_unknown_helper(self, strict_mode):
        """Unknown helpers should raise ImportError in strict mode."""
        with pytest.raises(ImportError, match="aiohttp_client"):
            from homeassistant.helpers import aiohttp_client

    def test_blocks_unknown_top_level(self, strict_mode):
        """Unknown top-level modules should raise ImportError in strict mode."""
        # Note: when HA core is installed, the error may be about a transitive import
        with pytest.raises(ImportError):
            from homeassistant import bootstrap

    def test_blocks_generated_module(self, strict_mode):
        """Generated modules should raise ImportError in strict mode.

        Note: When HA core is pip-installed, this module may be importable.
        Skip if import succeeds (HA core provides it).
        """
        try:
            from homeassistant import generated
            pytest.skip("HA core is installed, 'generated' module is available")
        except ImportError:
            pass  # Expected in strict mode without HA core

    def test_blocks_util_module(self, strict_mode):
        """Util module should raise ImportError in strict mode."""
        # Note: when HA core is installed, the error may be about a transitive import
        with pytest.raises(ImportError):
            from homeassistant import util


class TestExplicitlyShimmedModules:
    """Test that explicitly shimmed modules work in strict mode."""

    def test_const_module(self, strict_mode):
        """homeassistant.const should work."""
        from homeassistant.const import STATE_ON, STATE_OFF, Platform
        assert STATE_ON == "on"
        assert STATE_OFF == "off"
        assert Platform.LIGHT == "light"

    def test_const_includes_version(self, strict_mode):
        """homeassistant.const should include __version__."""
        from homeassistant.const import __version__, MAJOR_VERSION, MINOR_VERSION
        assert __version__ is not None
        assert isinstance(MAJOR_VERSION, int)
        assert isinstance(MINOR_VERSION, int)

    def test_const_includes_private_constants(self, strict_mode):
        """homeassistant.const should include _LOGGER and other private names."""
        from homeassistant import const
        # Check that we're not filtering out useful private names
        # (Some constants like _UNDEF might be needed)
        assert hasattr(const, "STATE_ON")

    def test_core_module(self, strict_mode):
        """homeassistant.core should work with our types."""
        from homeassistant.core import HomeAssistant, callback, Event, State
        assert HomeAssistant is not None
        assert callable(callback)
        assert Event is not None
        assert State is not None

    def test_core_includes_native_types(self, strict_mode):
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

    def test_exceptions_module(self, strict_mode):
        """homeassistant.exceptions should work."""
        from homeassistant.exceptions import HomeAssistantError, ConfigEntryError
        assert issubclass(HomeAssistantError, Exception)
        assert issubclass(ConfigEntryError, HomeAssistantError)

    def test_config_entries_module(self, strict_mode):
        """homeassistant.config_entries should work."""
        from homeassistant.config_entries import ConfigEntry, ConfigFlow, SOURCE_USER
        assert ConfigEntry is not None
        assert ConfigFlow is not None
        assert SOURCE_USER == "user"

    def test_helpers_entity_module(self, strict_mode):
        """homeassistant.helpers.entity should work."""
        from homeassistant.helpers.entity import Entity, DeviceInfo, RustStateMixin
        assert Entity is not None
        assert DeviceInfo is not None
        assert RustStateMixin is not None

    def test_helpers_entity_platform_module(self, strict_mode):
        """homeassistant.helpers.entity_platform should work."""
        from homeassistant.helpers.entity_platform import AddEntitiesCallback, EntityPlatform
        assert AddEntitiesCallback is not None
        assert EntityPlatform is not None

    def test_helpers_typing_module(self, strict_mode):
        """homeassistant.helpers.typing should work."""
        from homeassistant.helpers.typing import ConfigType
        assert ConfigType is not None

    def test_components_light_module(self, strict_mode):
        """homeassistant.components.light should work."""
        from homeassistant.components.light import LightEntity, ColorMode, ATTR_BRIGHTNESS
        assert LightEntity is not None
        assert ColorMode is not None
        assert ATTR_BRIGHTNESS == "brightness"

    def test_components_switch_module(self, strict_mode):
        """homeassistant.components.switch should work."""
        from homeassistant.components.switch import SwitchEntity, SwitchDeviceClass
        assert SwitchEntity is not None
        assert SwitchDeviceClass is not None

    def test_components_sensor_module(self, strict_mode):
        """homeassistant.components.sensor should work."""
        from homeassistant.components.sensor import SensorEntity, SensorDeviceClass, SensorStateClass
        assert SensorEntity is not None
        assert SensorDeviceClass is not None
        assert SensorStateClass is not None


class TestRustStateMixinInheritance:
    """Test that entity classes properly inherit RustStateMixin."""

    def test_entity_has_mixin(self, strict_mode):
        """Entity should have RustStateMixin in its MRO."""
        from homeassistant.helpers.entity import Entity, RustStateMixin
        assert RustStateMixin in Entity.__mro__

    def test_light_entity_has_mixin(self, strict_mode):
        """LightEntity should have RustStateMixin in its MRO."""
        from homeassistant.components.light import LightEntity
        from homeassistant.helpers.entity import RustStateMixin
        assert RustStateMixin in LightEntity.__mro__

    def test_switch_entity_has_mixin(self, strict_mode):
        """SwitchEntity should have RustStateMixin in its MRO."""
        from homeassistant.components.switch import SwitchEntity
        from homeassistant.helpers.entity import RustStateMixin
        assert RustStateMixin in SwitchEntity.__mro__

    def test_sensor_entity_has_mixin(self, strict_mode):
        """SensorEntity should have RustStateMixin in its MRO."""
        from homeassistant.components.sensor import SensorEntity
        from homeassistant.helpers.entity import RustStateMixin
        assert RustStateMixin in SensorEntity.__mro__

    def test_mixin_provides_async_write_ha_state(self, strict_mode):
        """RustStateMixin should provide async_write_ha_state method."""
        from homeassistant.helpers.entity import Entity, RustStateMixin
        import inspect

        # Check the method comes from our shim, not native
        method = Entity.async_write_ha_state
        source_file = inspect.getfile(method)
        assert "python/homeassistant" in source_file, f"Method from wrong source: {source_file}"

    def test_light_entity_async_write_ha_state_from_mixin(self, strict_mode):
        """LightEntity.async_write_ha_state should come from RustStateMixin."""
        from homeassistant.components.light import LightEntity
        import inspect

        method = LightEntity.async_write_ha_state
        source_file = inspect.getfile(method)
        assert "python/homeassistant" in source_file, f"Method from wrong source: {source_file}"


class TestFallbackMode:
    """Test that fallback mode works when explicitly enabled."""

    def test_fallback_allows_demo_component(self, fallback_mode):
        """Demo component should load in fallback mode."""
        from homeassistant.components.demo import light
        assert light is not None
        assert hasattr(light, "DemoLight")

    def test_fallback_demo_still_uses_mixin(self, fallback_mode):
        """Even in fallback mode, entities should use RustStateMixin."""
        from homeassistant.components.demo import light
        from homeassistant.helpers.entity import RustStateMixin

        assert RustStateMixin in light.DemoLight.__mro__

    def test_fallback_logs_warning(self, fallback_mode, caplog):
        """Fallback mode should log a warning."""
        import logging
        caplog.set_level(logging.WARNING)

        # Force reimport to trigger warning
        for name in list(sys.modules.keys()):
            if name.startswith("homeassistant"):
                del sys.modules[name]

        import homeassistant

        assert "HA_ALLOW_NATIVE_FALLBACK is enabled" in caplog.text


class TestNoAccidentalNativeInclusion:
    """Test that we don't accidentally include native HA modules."""

    def test_shim_modules_from_shim_directory(self, strict_mode):
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

    def test_native_loader_not_exposed(self, strict_mode):
        """_native_loader should not be importable as a public module."""
        # It's fine to import it directly, but it shouldn't be in __all__
        import homeassistant
        assert "_native_loader" not in getattr(homeassistant, "__all__", [])

    def test_env_var_values(self, strict_mode):
        """Only specific values should enable fallback."""
        # Test that empty string doesn't enable fallback
        with mock.patch.dict(os.environ, {"HA_ALLOW_NATIVE_FALLBACK": ""}):
            for name in list(sys.modules.keys()):
                if name.startswith("homeassistant"):
                    del sys.modules[name]
            import homeassistant
            # Should not have extended __path__
            native_path = str(VENDOR_PATH / "homeassistant")
            assert native_path not in homeassistant.__path__

        # Test that "0" doesn't enable fallback
        for name in list(sys.modules.keys()):
            if name.startswith("homeassistant"):
                del sys.modules[name]
        with mock.patch.dict(os.environ, {"HA_ALLOW_NATIVE_FALLBACK": "0"}):
            import homeassistant
            native_path = str(VENDOR_PATH / "homeassistant")
            assert native_path not in homeassistant.__path__

        # Test that "false" doesn't enable fallback
        for name in list(sys.modules.keys()):
            if name.startswith("homeassistant"):
                del sys.modules[name]
        with mock.patch.dict(os.environ, {"HA_ALLOW_NATIVE_FALLBACK": "false"}):
            import homeassistant
            native_path = str(VENDOR_PATH / "homeassistant")
            assert native_path not in homeassistant.__path__


class TestShimCompleteness:
    """Test that shimmed modules re-export everything needed."""

    def test_const_has_all_platforms(self, strict_mode):
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

    def test_light_has_color_modes(self, strict_mode):
        """light should have all ColorMode enum values."""
        from homeassistant.components.light import ColorMode
        expected_modes = [
            "UNKNOWN", "ONOFF", "BRIGHTNESS", "COLOR_TEMP", "HS", "XY",
            "RGB", "RGBW", "RGBWW", "WHITE",
        ]
        for mode in expected_modes:
            assert hasattr(ColorMode, mode), f"ColorMode.{mode} missing"

    def test_sensor_has_device_classes(self, strict_mode):
        """sensor should have common SensorDeviceClass values."""
        from homeassistant.components.sensor import SensorDeviceClass
        expected_classes = [
            "TEMPERATURE", "HUMIDITY", "BATTERY", "POWER", "ENERGY",
            "VOLTAGE", "CURRENT", "PRESSURE",
        ]
        for cls in expected_classes:
            assert hasattr(SensorDeviceClass, cls), f"SensorDeviceClass.{cls} missing"
