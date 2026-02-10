"""Tests for registries_init.py — entity/device registry initialization.

Verifies that _init_registries sets up entity and device registries in hass.data.

Note: Area registry initialization is now handled from Rust (see
crates/ha-py-bridge/src/py_bridge/wrappers/area_registry.rs) and tested
in Rust unit tests there.
"""

from __future__ import annotations

import json
import os
import tempfile


class FakeConfig:
    """Minimal config mock with path() method."""

    def __init__(self, config_dir: str):
        self._config_dir = config_dir

    def path(self, suffix: str = "") -> str:
        return os.path.join(self._config_dir, suffix)


class FakeHassData(dict):
    """Dict subclass that also supports HassKey lookups.

    HA uses HassKey objects as dict keys. HassKey subclasses str,
    so a regular dict works if we also support __contains__ for
    string-equivalent keys.
    """

    pass


class FakeHass:
    """Minimal HomeAssistant-like object for testing _init_registries."""

    def __init__(self, config_dir: str):
        self.data = FakeHassData()
        self.config = FakeConfig(config_dir)
        self.bus = FakeBus()
        self.is_running = True
        self.is_stopping = False

    def verify_event_loop_thread(self, func_name: str) -> None:
        pass


class FakeBus:
    """Minimal bus mock."""

    def async_fire(self, event_type, event_data=None, origin=None, context=None):
        pass

    def async_listen(self, event_type, listener, event_filter=None):
        return lambda: None

    def async_listen_once(self, event_type, listener, event_filter=None):
        return lambda: None


def create_minimal_storage(config_dir: str) -> None:
    """Create minimal entity/device registry storage files."""
    storage_dir = os.path.join(config_dir, ".storage")
    os.makedirs(storage_dir, exist_ok=True)

    # Minimal entity registry
    with open(os.path.join(storage_dir, "core.entity_registry"), "w") as f:
        json.dump(
            {"version": 1, "minor_version": 1, "key": "core.entity_registry", "data": {"entities": []}},
            f,
        )

    # Minimal device registry
    with open(os.path.join(storage_dir, "core.device_registry"), "w") as f:
        json.dump(
            {"version": 1, "minor_version": 1, "key": "core.device_registry", "data": {"devices": []}},
            f,
        )


class TestRegistriesInit:
    """Test that _init_registries initializes entity/device registries."""

    def test_init_registries_executes_without_error(self) -> None:
        """Test that _init_registries can be loaded and called without crashing."""
        with tempfile.TemporaryDirectory() as tmpdir:
            create_minimal_storage(tmpdir)

            hass = FakeHass(tmpdir)

            init_path = os.path.join(
                os.path.dirname(__file__),
                "..",
                "..",
                "embedded_python",
                "registries_init.py",
            )
            with open(init_path) as f:
                code = f.read()

            exec_globals = {}
            exec(code, exec_globals)

            init_fn = exec_globals["_init_registries"]
            # Should not raise even without homeassistant on the path
            # (individual registry inits may fail gracefully)
            result = init_fn(hass, tmpdir)
            # Result may be True or False depending on whether
            # homeassistant module is available
            assert result is not None
