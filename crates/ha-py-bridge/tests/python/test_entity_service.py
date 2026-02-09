"""Tests for entity_service.py fallback behavior.

When an entity exists in the state store but NOT in the Python _entity_registry,
service calls should fall back to direct state store manipulation.
"""

from __future__ import annotations

import types


class MockStates:
    """Mock states wrapper that tracks get/set calls."""

    def __init__(self):
        self._states = {}

    def get(self, entity_id):
        """Get entity state."""
        entry = self._states.get(entity_id)
        if entry is None:
            return None
        return entry

    def set(self, entity_id, state, attributes=None):
        """Set entity state."""
        self._states[entity_id] = {
            "entity_id": entity_id,
            "state": state,
            "attributes": attributes or {},
        }


class TestEntityServiceFallback:
    """Test service calls on entities not in Python _entity_registry."""

    def _setup_globals(self):
        """Create a globals dict that simulates CONFIG_ENTRIES_GLOBALS."""
        import logging

        states = MockStates()
        # Pre-populate a light entity in the state store
        states.set("light.bedroom", "off", {"friendly_name": "Bedroom"})

        hass = types.SimpleNamespace()
        hass.states = states

        return {
            "_entity_registry": {},  # Empty — entity not registered via platform setup
            "_hass": hass,
            "_LOGGER": logging.getLogger("test_entity_service"),
        }

    def _load_entity_service(self, globals_dict):
        """Execute entity_service.py in the given globals context."""
        import os

        entity_service_path = os.path.join(
            os.path.dirname(__file__),
            "..",
            "..",
            "embedded_python",
            "entity_service.py",
        )
        with open(entity_service_path) as f:
            code = f.read()
        exec(code, globals_dict)
        return globals_dict["_call_entity_service_sync"]

    def test_turn_on_entity_not_in_registry_falls_back_to_state_store(self):
        """Service call on entity NOT in _entity_registry should update state store."""
        g = self._setup_globals()
        call_fn = self._load_entity_service(g)

        # Entity is NOT in _entity_registry but IS in state store
        assert "light.bedroom" not in g["_entity_registry"]
        assert g["_hass"].states.get("light.bedroom") is not None

        # Call turn_on — should fall back to direct state manipulation
        result = call_fn("light.bedroom", "turn_on", {})
        assert result is True

        # State should now be "on"
        state = g["_hass"].states.get("light.bedroom")
        assert state["state"] == "on"

    def test_turn_off_entity_not_in_registry_falls_back_to_state_store(self):
        """Service call turn_off on state-store-only entity should set state to off."""
        g = self._setup_globals()
        # Start with light on
        g["_hass"].states.set("light.bedroom", "on", {"friendly_name": "Bedroom"})
        call_fn = self._load_entity_service(g)

        result = call_fn("light.bedroom", "turn_off", {})
        assert result is True
        assert g["_hass"].states.get("light.bedroom")["state"] == "off"

    def test_toggle_entity_not_in_registry_toggles_state(self):
        """Toggle on state-store-only entity should flip the state."""
        g = self._setup_globals()
        call_fn = self._load_entity_service(g)

        # Start off, toggle should turn on
        result = call_fn("light.bedroom", "toggle", {})
        assert result is True
        assert g["_hass"].states.get("light.bedroom")["state"] == "on"

        # Toggle again should turn off
        # Need to reload since function is redefined each exec
        call_fn2 = self._load_entity_service(g)
        result = call_fn2("light.bedroom", "toggle", {})
        assert result is True
        assert g["_hass"].states.get("light.bedroom")["state"] == "off"

    def test_entity_in_registry_still_uses_registry(self):
        """When entity IS in _entity_registry, use it (existing behavior)."""
        g = self._setup_globals()
        call_fn = self._load_entity_service(g)

        # Create a mock entity in the registry
        entity = types.SimpleNamespace()
        entity.entity_id = "light.bedroom"
        entity._attr_is_on = False
        g["_entity_registry"]["light.bedroom"] = entity

        result = call_fn("light.bedroom", "turn_on", {})
        assert result is True
        assert entity._attr_is_on is True

    def test_entity_not_in_registry_or_state_store_returns_false(self):
        """Entity in neither registry nor state store should return False."""
        g = self._setup_globals()
        call_fn = self._load_entity_service(g)

        result = call_fn("light.nonexistent", "turn_on", {})
        assert result is False

    def test_lock_unlock_via_state_store(self):
        """Lock/unlock services via state store fallback."""
        g = self._setup_globals()
        g["_hass"].states.set("lock.front_door", "locked", {})
        call_fn = self._load_entity_service(g)

        result = call_fn("lock.front_door", "unlock", {})
        assert result is True
        assert g["_hass"].states.get("lock.front_door")["state"] == "unlocked"

        call_fn2 = self._load_entity_service(g)
        result = call_fn2("lock.front_door", "lock", {})
        assert result is True
        assert g["_hass"].states.get("lock.front_door")["state"] == "locked"
