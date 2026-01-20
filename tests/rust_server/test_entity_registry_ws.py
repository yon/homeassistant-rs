"""Tests for entity registry WebSocket API against our Rust server.

These tests verify that our Rust implementation of the entity registry
WebSocket commands matches the expected HA behavior.
"""

import pytest


class TestEntityRegistryList:
    """Tests for config/entity_registry/list command."""

    @pytest.mark.asyncio
    async def test_list_entities(self, rust_ws_client) -> None:
        """Test listing entities."""
        response = await rust_ws_client.call("config/entity_registry/list")

        assert response["success"] is True
        assert response["type"] == "result"
        assert isinstance(response["result"], list)

    @pytest.mark.asyncio
    async def test_list_entities_entry_fields(self, rust_ws_client) -> None:
        """Test each entity entry has required fields."""
        response = await rust_ws_client.call("config/entity_registry/list")

        required_fields = [
            "area_id",
            "categories",
            "config_entry_id",
            "created_at",
            "device_id",
            "disabled_by",
            "entity_category",
            "entity_id",
            "has_entity_name",
            "hidden_by",
            "icon",
            "id",
            "labels",
            "modified_at",
            "name",
            "options",
            "original_name",
            "platform",
            "translation_key",
            "unique_id",
        ]

        for entry in response["result"]:
            for field in required_fields:
                assert field in entry, f"Missing field: {field}"

    @pytest.mark.asyncio
    async def test_list_entities_field_types(self, rust_ws_client) -> None:
        """Test that entity entry fields have correct types."""
        response = await rust_ws_client.call("config/entity_registry/list")

        for entry in response["result"]:
            # Required string fields
            assert isinstance(entry["entity_id"], str)
            assert isinstance(entry["id"], str)
            assert isinstance(entry["platform"], str)
            assert isinstance(entry["unique_id"], str)

            # Boolean fields
            assert isinstance(entry["has_entity_name"], bool)

            # Array fields
            assert isinstance(entry["labels"], list)

            # Object fields
            assert isinstance(entry["categories"], dict)
            assert isinstance(entry["options"], dict)

            # Nullable string fields
            for field in ["area_id", "config_entry_id", "device_id", "disabled_by",
                         "entity_category", "hidden_by", "icon", "name",
                         "original_name", "translation_key"]:
                assert entry[field] is None or isinstance(entry[field], str), \
                    f"{field} should be string or null"

            # Timestamp fields
            for field in ["created_at", "modified_at"]:
                assert isinstance(entry[field], (int, float)), f"{field} should be number"


class TestEntityRegistryGet:
    """Tests for config/entity_registry/get command."""

    @pytest.mark.asyncio
    async def test_get_entity_nonexistent(self, rust_ws_client) -> None:
        """Test getting a non-existent entity."""
        response = await rust_ws_client.call(
            "config/entity_registry/get",
            entity_id="nonexistent.entity",
        )

        # Should either return null/error or indicate not found
        if response["success"]:
            assert response["result"] is None or "entity_id" not in response["result"]


class TestEntityRegistryUpdate:
    """Tests for config/entity_registry/update command."""

    @pytest.mark.skip(reason="config/entity_registry/update not yet fully implemented")
    @pytest.mark.asyncio
    async def test_update_entity_not_found(self, rust_ws_client) -> None:
        """Test updating a non-existent entity returns error."""
        response = await rust_ws_client.call(
            "config/entity_registry/update",
            entity_id="nonexistent.entity",
            name="New Name",
        )

        assert response["success"] is False


class TestEntityRegistryMultipleCalls:
    """Tests for making multiple entity registry calls."""

    @pytest.mark.asyncio
    async def test_multiple_list_calls(self, rust_ws_client) -> None:
        """Test making multiple list calls returns consistent data."""
        response1 = await rust_ws_client.call("config/entity_registry/list")
        response2 = await rust_ws_client.call("config/entity_registry/list")

        assert response1["success"] is True
        assert response2["success"] is True
        assert response1["result"] == response2["result"]
