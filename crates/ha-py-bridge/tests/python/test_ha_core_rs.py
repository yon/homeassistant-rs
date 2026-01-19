"""Tests for ha_core_rs Rust extension module.

These tests verify that the Rust implementation matches Python Home Assistant's
core component behavior.
"""

from __future__ import annotations

import pytest

from ha_core_rs import (
    HomeAssistant,
    EventBus,
    ServiceRegistry,
    StateMachine,
    Context,
    EntityId,
    State,
    Event,
)


class TestEntityId:
    """Test EntityId validation and parsing."""

    def test_valid_entity_id(self) -> None:
        """Test valid entity ID creation."""
        eid = EntityId("light.living_room")
        assert eid.domain == "light"
        assert eid.object_id == "living_room"
        assert str(eid) == "light.living_room"

    def test_entity_id_with_underscores(self) -> None:
        """Test entity ID with underscores."""
        eid = EntityId("sensor.outdoor_temperature_1")
        assert eid.domain == "sensor"
        assert eid.object_id == "outdoor_temperature_1"

    def test_invalid_entity_id_no_dot(self) -> None:
        """Test that entity ID without dot raises ValueError."""
        with pytest.raises(ValueError, match="separator"):
            EntityId("invalid")

    def test_invalid_entity_id_empty(self) -> None:
        """Test that empty entity ID raises ValueError."""
        with pytest.raises(ValueError):
            EntityId("")

    def test_invalid_entity_id_only_dot(self) -> None:
        """Test that entity ID with only dot raises ValueError."""
        with pytest.raises(ValueError):
            EntityId(".")

    def test_invalid_entity_id_empty_domain(self) -> None:
        """Test that entity ID with empty domain raises ValueError."""
        with pytest.raises(ValueError):
            EntityId(".object_id")

    def test_invalid_entity_id_empty_object_id(self) -> None:
        """Test that entity ID with empty object_id raises ValueError."""
        with pytest.raises(ValueError):
            EntityId("domain.")


class TestContext:
    """Test Context creation and properties."""

    def test_default_context(self) -> None:
        """Test default context has generated ID."""
        ctx = Context()
        assert ctx.id is not None
        assert len(ctx.id) == 26  # ULID length
        assert ctx.user_id is None
        assert ctx.parent_id is None

    def test_context_with_user_id(self) -> None:
        """Test context with user ID."""
        ctx = Context(user_id="admin")
        assert ctx.user_id == "admin"
        assert ctx.parent_id is None

    def test_context_with_parent_id(self) -> None:
        """Test context with parent ID."""
        ctx = Context(parent_id="parent123")
        assert ctx.parent_id == "parent123"

    def test_context_with_all_fields(self) -> None:
        """Test context with all fields."""
        ctx = Context(user_id="user1", parent_id="parent123")
        assert ctx.user_id == "user1"
        assert ctx.parent_id == "parent123"

    def test_context_unique_ids(self) -> None:
        """Test each context gets a unique ID."""
        ctx1 = Context()
        ctx2 = Context()
        assert ctx1.id != ctx2.id


class TestStateMachine:
    """Test StateMachine state management."""

    def test_set_and_get_state(self) -> None:
        """Test setting and getting state."""
        hass = HomeAssistant()
        hass.states.set("light.test", "on", {"brightness": 255})

        state = hass.states.get("light.test")
        assert state is not None
        # entity_id returns EntityId object, compare with str()
        assert str(state.entity_id) == "light.test"
        assert state.state == "on"
        assert state.attributes == {"brightness": 255}

    def test_get_nonexistent_state(self) -> None:
        """Test getting state for entity that doesn't exist."""
        hass = HomeAssistant()
        state = hass.states.get("nonexistent.entity")
        assert state is None

    def test_state_update(self) -> None:
        """Test updating existing state."""
        hass = HomeAssistant()
        hass.states.set("sensor.temp", "20", {"unit": "°C"})
        hass.states.set("sensor.temp", "22", {"unit": "°C"})

        state = hass.states.get("sensor.temp")
        assert state.state == "22"

    def test_entity_ids_by_domain(self) -> None:
        """Test getting entity IDs filtered by domain."""
        hass = HomeAssistant()
        hass.states.set("light.living_room", "on", {})
        hass.states.set("light.bedroom", "off", {})
        hass.states.set("sensor.temperature", "22", {})

        light_ids = hass.states.entity_ids("light")
        assert "light.living_room" in light_ids
        assert "light.bedroom" in light_ids
        assert "sensor.temperature" not in light_ids

        sensor_ids = hass.states.entity_ids("sensor")
        assert "sensor.temperature" in sensor_ids
        assert len(sensor_ids) == 1

    def test_remove_state(self) -> None:
        """Test removing a state."""
        hass = HomeAssistant()
        hass.states.set("light.test", "on", {})

        # Remove returns the old state
        removed = hass.states.remove("light.test")
        assert removed is not None
        assert str(removed.entity_id) == "light.test"

        # Now it should be gone
        assert hass.states.get("light.test") is None

    def test_remove_nonexistent_state(self) -> None:
        """Test removing state that doesn't exist."""
        hass = HomeAssistant()
        result = hass.states.remove("nonexistent.entity")
        assert result is None

    def test_state_count(self) -> None:
        """Test state count via __len__."""
        hass = HomeAssistant()
        assert len(hass.states) == 0

        hass.states.set("light.one", "on", {})
        assert len(hass.states) == 1

        hass.states.set("light.two", "off", {})
        assert len(hass.states) == 2

        hass.states.remove("light.one")
        assert len(hass.states) == 1

    def test_state_timestamps(self) -> None:
        """Test that states have timestamps."""
        hass = HomeAssistant()
        hass.states.set("sensor.test", "100", {})

        state = hass.states.get("sensor.test")
        assert state.last_changed is not None
        assert state.last_updated is not None

        # Should be ISO format with timezone
        assert "T" in state.last_changed
        assert "+" in state.last_changed or "Z" in state.last_changed


class TestEventBus:
    """Test EventBus functionality."""

    def test_create_event_bus(self) -> None:
        """Test creating an event bus."""
        bus = EventBus()
        assert bus.listener_count() == 0

    def test_fire_event(self) -> None:
        """Test firing an event."""
        bus = EventBus()
        # Fire doesn't require Tokio runtime
        bus.fire("test_event", {"key": "value"})

    def test_async_fire_alias(self) -> None:
        """Test async_fire is an alias for fire."""
        bus = EventBus()
        bus.async_fire("test_event", {})


class TestServiceRegistry:
    """Test ServiceRegistry functionality."""

    def test_create_registry(self) -> None:
        """Test creating a service registry."""
        services = ServiceRegistry()
        assert len(services) == 0

    def test_register_service(self) -> None:
        """Test registering a service."""
        services = ServiceRegistry()

        def handler(call):
            return None

        services.register("test", "my_service", handler)
        assert services.has_service("test", "my_service")
        assert len(services) == 1

    def test_has_service_false(self) -> None:
        """Test has_service returns False for unregistered service."""
        services = ServiceRegistry()
        assert not services.has_service("test", "nonexistent")

    def test_get_service(self) -> None:
        """Test getting service description."""
        services = ServiceRegistry()
        services.register("light", "turn_on", lambda c: None)

        desc = services.get_service("light", "turn_on")
        assert desc["domain"] == "light"
        assert desc["service"] == "turn_on"

    def test_get_service_nonexistent(self) -> None:
        """Test getting nonexistent service returns None."""
        services = ServiceRegistry()
        assert services.get_service("test", "nonexistent") is None

    def test_domains(self) -> None:
        """Test getting list of domains."""
        services = ServiceRegistry()
        services.register("light", "turn_on", lambda c: None)
        services.register("switch", "toggle", lambda c: None)

        domains = services.domains()
        assert "light" in domains
        assert "switch" in domains

    def test_domain_services(self) -> None:
        """Test getting services for a domain."""
        services = ServiceRegistry()
        services.register("light", "turn_on", lambda c: None)
        services.register("light", "turn_off", lambda c: None)
        services.register("switch", "toggle", lambda c: None)

        light_services = services.domain_services("light")
        assert len(light_services) == 2

    def test_all_services(self) -> None:
        """Test getting all services."""
        services = ServiceRegistry()
        services.register("light", "turn_on", lambda c: None)
        services.register("switch", "toggle", lambda c: None)

        all_svcs = services.all_services()
        assert "light" in all_svcs
        assert "switch" in all_svcs

    def test_unregister_service(self) -> None:
        """Test unregistering a service."""
        services = ServiceRegistry()
        services.register("test", "my_service", lambda c: None)

        result = services.unregister("test", "my_service")
        assert result is True
        assert not services.has_service("test", "my_service")

    def test_unregister_nonexistent(self) -> None:
        """Test unregistering nonexistent service returns False."""
        services = ServiceRegistry()
        result = services.unregister("test", "nonexistent")
        assert result is False

    def test_unregister_domain(self) -> None:
        """Test unregistering all services for a domain."""
        services = ServiceRegistry()
        services.register("light", "turn_on", lambda c: None)
        services.register("light", "turn_off", lambda c: None)
        services.register("switch", "toggle", lambda c: None)

        removed = services.unregister_domain("light")
        assert removed == 2
        assert len(services) == 1
        assert not services.has_service("light", "turn_on")
        assert services.has_service("switch", "toggle")

    def test_register_with_schema(self) -> None:
        """Test registering service with schema."""
        services = ServiceRegistry()
        schema = {
            "type": "object",
            "properties": {
                "brightness": {"type": "integer"}
            }
        }
        services.register("light", "set_brightness", lambda c: None, schema=schema)
        assert services.has_service("light", "set_brightness")

    def test_register_supports_response_none(self) -> None:
        """Test registering service with supports_response=none."""
        services = ServiceRegistry()
        services.register("test", "no_response", lambda c: None, supports_response="none")
        assert services.has_service("test", "no_response")

    def test_register_supports_response_only(self) -> None:
        """Test registering service with supports_response=only."""
        services = ServiceRegistry()
        services.register("test", "only_response", lambda c: {"result": "ok"}, supports_response="only")
        assert services.has_service("test", "only_response")

    def test_register_supports_response_optional(self) -> None:
        """Test registering service with supports_response=optional."""
        services = ServiceRegistry()
        services.register("test", "optional", lambda c: None, supports_response="optional")
        assert services.has_service("test", "optional")

    def test_register_supports_response_invalid(self) -> None:
        """Test that invalid supports_response raises ValueError."""
        services = ServiceRegistry()
        with pytest.raises(ValueError):
            services.register("test", "bad", lambda c: None, supports_response="invalid")

    def test_register_non_callable_raises(self) -> None:
        """Test that non-callable handler raises TypeError."""
        services = ServiceRegistry()
        with pytest.raises(TypeError, match="callable"):
            services.register("test", "bad", "not a function")


class TestHomeAssistant:
    """Test HomeAssistant main class."""

    def test_create_homeassistant(self) -> None:
        """Test creating a HomeAssistant instance."""
        hass = HomeAssistant()
        assert hass is not None

    def test_homeassistant_bus(self) -> None:
        """Test HomeAssistant has an event bus."""
        hass = HomeAssistant()
        assert hass.bus is not None

    def test_homeassistant_states(self) -> None:
        """Test HomeAssistant has a state machine."""
        hass = HomeAssistant()
        assert hass.states is not None

    def test_homeassistant_services(self) -> None:
        """Test HomeAssistant has a service registry."""
        hass = HomeAssistant()
        assert hass.services is not None

    def test_is_running(self) -> None:
        """Test is_running property."""
        hass = HomeAssistant()
        assert hass.is_running is True

    def test_is_stopping(self) -> None:
        """Test is_stopping property."""
        hass = HomeAssistant()
        assert hass.is_stopping is False

    def test_pending_task_count(self) -> None:
        """Test pending_task_count method."""
        hass = HomeAssistant()
        assert hass.pending_task_count() == 0

    def test_homeassistant_repr(self) -> None:
        """Test HomeAssistant repr."""
        hass = HomeAssistant()
        hass.states.set("light.test", "on", {})
        hass.services.register("test", "svc", lambda c: None)

        repr_str = repr(hass)
        assert "HomeAssistant" in repr_str
        assert "entities=1" in repr_str
        assert "services=1" in repr_str


class TestState:
    """Test State object properties."""

    def test_state_entity_id(self) -> None:
        """Test State entity_id property."""
        hass = HomeAssistant()
        hass.states.set("light.test", "on", {})
        state = hass.states.get("light.test")
        # entity_id returns EntityId object
        assert str(state.entity_id) == "light.test"
        assert state.entity_id.domain == "light"
        assert state.entity_id.object_id == "test"

    def test_state_value(self) -> None:
        """Test State state property."""
        hass = HomeAssistant()
        hass.states.set("light.test", "on", {})
        state = hass.states.get("light.test")
        assert state.state == "on"

    def test_state_attributes(self) -> None:
        """Test State attributes property."""
        hass = HomeAssistant()
        hass.states.set("light.test", "on", {"brightness": 255, "color": "red"})
        state = hass.states.get("light.test")
        assert state.attributes == {"brightness": 255, "color": "red"}

    def test_state_attributes_empty(self) -> None:
        """Test State with empty attributes."""
        hass = HomeAssistant()
        hass.states.set("light.test", "on", {})
        state = hass.states.get("light.test")
        assert state.attributes == {}

    def test_state_repr(self) -> None:
        """Test State repr."""
        hass = HomeAssistant()
        hass.states.set("light.test", "on", {})
        state = hass.states.get("light.test")
        repr_str = repr(state)
        assert "light.test" in repr_str
        assert "on" in repr_str
