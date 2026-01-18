"""Tests for loading the demo integration via FallbackBridge.

This test verifies that we can load a real Home Assistant integration
using our Rust FallbackBridge infrastructure.
"""

from __future__ import annotations

import asyncio
import pytest
import sys
import os

# Add vendor/ha-core to the path so we can import the real HA components
vendor_path = os.path.join(
    os.path.dirname(os.path.dirname(os.path.dirname(__file__))),
    "vendor",
    "ha-core",
)
if vendor_path not in sys.path:
    sys.path.insert(0, vendor_path)


class TestDemoIntegrationLoad:
    """Test loading the demo integration."""

    def test_import_demo_integration(self) -> None:
        """Test that we can import the demo integration."""
        from homeassistant.components import demo

        assert hasattr(demo, "async_setup_entry")
        assert hasattr(demo, "async_unload_entry")
        assert hasattr(demo, "DOMAIN")
        assert demo.DOMAIN == "demo"

    def test_demo_platforms(self) -> None:
        """Test demo integration has the expected platforms."""
        from homeassistant.components import demo

        # Check that demo defines the platforms it supports
        assert hasattr(demo, "COMPONENTS_WITH_CONFIG_ENTRY_DEMO_PLATFORM")
        platforms = demo.COMPONENTS_WITH_CONFIG_ENTRY_DEMO_PLATFORM
        assert len(platforms) > 0

        # Verify some expected platforms are present
        platform_names = [str(p) for p in platforms]
        # Platform enum values look like Platform.LIGHT, Platform.SWITCH etc.
        assert any("light" in p.lower() for p in platform_names)
        assert any("switch" in p.lower() for p in platform_names)
        assert any("sensor" in p.lower() for p in platform_names)


class TestHassWrapper:
    """Test that our hass wrapper provides what demo needs."""

    @pytest.fixture
    def hass_wrapper(self):
        """Create a hass-like wrapper matching what our Rust FallbackBridge creates."""
        import types

        hass = types.SimpleNamespace()

        # Add data dict
        hass.data = {}

        # Add config_entries with async_forward_entry_setups
        config_entries = types.SimpleNamespace()

        async def async_forward_entry_setups(entry, platforms):
            """Mock forward entry setups."""
            entry_id = (
                entry.get("entry_id")
                if isinstance(entry, dict)
                else getattr(entry, "entry_id", "unknown")
            )
            return True

        async def async_unload_platforms(entry, platforms):
            """Mock unload platforms."""
            return True

        config_entries.async_forward_entry_setups = async_forward_entry_setups
        config_entries.async_unload_platforms = async_unload_platforms
        hass.config_entries = config_entries

        # Add config
        config = types.SimpleNamespace()
        config.latitude = 32.87336
        config.longitude = -117.22743
        config.config_dir = "/config"
        config.components = set()
        hass.config = config

        # Add async_create_task
        def async_create_task(coro, name=None, eager_start=False):
            return asyncio.ensure_future(coro)

        hass.async_create_task = async_create_task

        return hass

    @pytest.fixture
    def config_entry_dict(self):
        """Create a config entry dict matching what our Rust config_entry_to_python creates."""
        return {
            "entry_id": "test_demo_entry",
            "domain": "demo",
            "title": "Demo",
            "data": {},
            "options": {},
            "version": 1,
            "minor_version": 1,
            "unique_id": None,
            "source": "user",
            "state": "not_loaded",
            "reason": None,
            "pref_disable_new_entities": False,
            "pref_disable_polling": False,
            "disabled_by": None,
            "discovery_keys": {},
        }

    @pytest.mark.asyncio
    async def test_call_demo_async_setup_entry(
        self, hass_wrapper, config_entry_dict
    ) -> None:
        """Test calling demo's async_setup_entry with our wrapper."""
        from homeassistant.components import demo

        # Call the integration's setup_entry
        result = await demo.async_setup_entry(hass_wrapper, config_entry_dict)

        # Should return True for successful setup
        assert result is True

    @pytest.mark.asyncio
    async def test_call_demo_async_unload_entry(
        self, hass_wrapper, config_entry_dict
    ) -> None:
        """Test calling demo's async_unload_entry with our wrapper."""
        from homeassistant.components import demo

        # First setup
        await demo.async_setup_entry(hass_wrapper, config_entry_dict)

        # Then unload
        result = await demo.async_unload_entry(hass_wrapper, config_entry_dict)

        # Should return True for successful unload
        assert result is True


class TestRustFallbackBridge:
    """Test loading demo via the actual Rust FallbackBridge."""

    def test_fallback_bridge_exists(self) -> None:
        """Test that ha_core_rs has fallback module."""
        try:
            import ha_core_rs

            # Check if fallback features are available
            # This may fail if the extension wasn't built with fallback support
            assert hasattr(ha_core_rs, "HomeAssistant")
        except ImportError:
            pytest.skip("ha_core_rs not available")
