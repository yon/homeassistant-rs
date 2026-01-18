#!/usr/bin/env python3
"""Test loading the demo integration with our hass wrapper.

This test verifies that our Python hass wrapper provides enough
compatibility to load the demo integration.
"""

import asyncio
import logging
import sys
from types import SimpleNamespace

# Set up logging
logging.basicConfig(level=logging.INFO)
_LOGGER = logging.getLogger(__name__)


def create_mock_hass():
    """Create a mock hass object similar to what our Rust wrapper creates."""
    hass = SimpleNamespace()

    # Data dict
    hass.data = {}

    # Bus with async_fire
    hass.bus = SimpleNamespace()

    async def async_fire(event_type, event_data=None, origin=None, context=None):
        _LOGGER.info(f"Event fired: {event_type}")

    async def async_listen(event_type, listener):
        _LOGGER.info(f"Event listener registered: {event_type}")
        return lambda: None  # Return unsubscribe function

    hass.bus.async_fire = async_fire
    hass.bus.async_listen = async_listen

    # States
    hass.states = SimpleNamespace()

    def get(entity_id):
        return None

    async def async_set(entity_id, new_state, attributes=None, force_update=False, context=None):
        _LOGGER.info(f"State set: {entity_id} = {new_state}")

    hass.states.get = get
    hass.states.async_set = async_set

    # Services
    hass.services = SimpleNamespace()

    async def async_call(domain, service, service_data=None, blocking=False, context=None, target=None):
        _LOGGER.info(f"Service called: {domain}.{service}")

    async def async_register(domain, service, service_func, schema=None):
        _LOGGER.info(f"Service registered: {domain}.{service}")

    hass.services.async_call = async_call
    hass.services.async_register = async_register

    # Config entries - this is what the demo integration needs
    hass.config_entries = SimpleNamespace()
    hass.config_entries.flow = SimpleNamespace()

    _loaded_platforms = {}

    async def async_forward_entry_setups(entry, platforms):
        entry_id = getattr(entry, "entry_id", "unknown")
        domain = getattr(entry, "domain", "unknown")
        _LOGGER.info(f"Forward entry setup for {domain} ({entry_id}): {list(platforms)}")
        if entry_id not in _loaded_platforms:
            _loaded_platforms[entry_id] = set()
        for platform in platforms:
            platform_name = str(platform).split(".")[-1] if "." in str(platform) else str(platform)
            _loaded_platforms[entry_id].add(platform_name)
        await asyncio.sleep(0)

    async def async_unload_platforms(entry, platforms):
        entry_id = getattr(entry, "entry_id", "unknown")
        domain = getattr(entry, "domain", "unknown")
        _LOGGER.info(f"Unload platforms for {domain} ({entry_id}): {list(platforms)}")
        return True

    async def async_init(domain, *, context=None, data=None):
        _LOGGER.info(f"Config flow init for {domain}")
        return {"flow_id": f"{domain}_flow_1", "type": "form"}

    hass.config_entries.async_forward_entry_setups = async_forward_entry_setups
    hass.config_entries.async_unload_platforms = async_unload_platforms
    hass.config_entries.flow.async_init = async_init

    # Config with location
    hass.config = SimpleNamespace()
    hass.config.config_dir = "/config"
    hass.config.latitude = 32.87336
    hass.config.longitude = -117.22743
    hass.config.elevation = 0
    hass.config.time_zone = "UTC"
    hass.config.units = "metric"
    hass.config.location_name = "Home"
    hass.config.components = set()
    hass.config.internal_url = None
    hass.config.external_url = None

    # Event loop
    hass.loop = asyncio.get_event_loop()

    # async_create_task
    def async_create_task(coro, name=None, eager_start=False):
        return hass.loop.create_task(coro, name=name)

    hass.async_create_task = async_create_task

    return hass


def create_mock_config_entry():
    """Create a mock config entry for the demo integration."""
    entry = SimpleNamespace()
    entry.entry_id = "demo_test_entry"
    entry.domain = "demo"
    entry.title = "Demo"
    entry.data = {}
    entry.options = {}
    entry.version = 1
    entry.minor_version = 1
    entry.source = "user"
    entry.state = "not_loaded"
    entry.unique_id = "demo_unique"
    entry.disabled_by = None
    entry.pref_disable_new_entities = False
    entry.pref_disable_polling = False
    entry.discovery_keys = {}

    # Add setup_lock that some integrations expect
    entry.setup_lock = asyncio.Lock()

    return entry


async def test_demo_setup_entry():
    """Test that our mock hass can call demo's async_setup_entry."""
    _LOGGER.info("=" * 60)
    _LOGGER.info("Testing demo integration async_setup_entry")
    _LOGGER.info("=" * 60)

    # Create mock objects
    hass = create_mock_hass()
    entry = create_mock_config_entry()

    # Import the demo integration
    try:
        from homeassistant.components.demo import async_setup_entry
        _LOGGER.info("Successfully imported demo.async_setup_entry")
    except ImportError as e:
        _LOGGER.error(f"Failed to import demo integration: {e}")
        return False

    # Call async_setup_entry
    try:
        result = await async_setup_entry(hass, entry)
        _LOGGER.info(f"async_setup_entry returned: {result}")
        return result
    except Exception as e:
        _LOGGER.error(f"async_setup_entry failed: {e}")
        import traceback
        traceback.print_exc()
        return False


async def test_demo_unload_entry():
    """Test that our mock hass can call demo's async_unload_entry."""
    _LOGGER.info("=" * 60)
    _LOGGER.info("Testing demo integration async_unload_entry")
    _LOGGER.info("=" * 60)

    # Create mock objects
    hass = create_mock_hass()
    entry = create_mock_config_entry()

    # Import the demo integration
    try:
        from homeassistant.components.demo import async_unload_entry
        _LOGGER.info("Successfully imported demo.async_unload_entry")
    except ImportError as e:
        _LOGGER.error(f"Failed to import demo integration: {e}")
        return False

    # Call async_unload_entry
    try:
        result = await async_unload_entry(hass, entry)
        _LOGGER.info(f"async_unload_entry returned: {result}")
        return result
    except Exception as e:
        _LOGGER.error(f"async_unload_entry failed: {e}")
        import traceback
        traceback.print_exc()
        return False


async def main():
    """Run all tests."""
    setup_ok = await test_demo_setup_entry()
    unload_ok = await test_demo_unload_entry()

    _LOGGER.info("=" * 60)
    _LOGGER.info("Test Results:")
    _LOGGER.info(f"  async_setup_entry: {'PASS' if setup_ok else 'FAIL'}")
    _LOGGER.info(f"  async_unload_entry: {'PASS' if unload_ok else 'FAIL'}")
    _LOGGER.info("=" * 60)

    return setup_ok and unload_ok


if __name__ == "__main__":
    success = asyncio.run(main())
    sys.exit(0 if success else 1)
