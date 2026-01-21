"""Tests for area, floor, and label registry WebSocket APIs against our Rust server.

These tests verify that our Rust implementation of the registry
WebSocket commands matches the expected HA behavior.
"""

import pytest


class TestAreaRegistry:
    """Tests for area registry WebSocket commands."""

    @pytest.mark.asyncio
    async def test_list_areas(self, rust_ws_client) -> None:
        """Test listing areas."""
        response = await rust_ws_client.call("config/area_registry/list")

        assert response["success"] is True
        assert response["type"] == "result"
        assert isinstance(response["result"], list)

    @pytest.mark.asyncio
    async def test_list_areas_entry_fields(self, rust_ws_client) -> None:
        """Test each area entry has required fields."""
        response = await rust_ws_client.call("config/area_registry/list")

        required_fields = [
            "aliases",
            "area_id",
            "created_at",
            "floor_id",
            "humidity_entity_id",
            "icon",
            "labels",
            "modified_at",
            "name",
            "picture",
            "temperature_entity_id",
        ]

        for entry in response["result"]:
            for field in required_fields:
                assert field in entry, f"Missing field: {field}"

    @pytest.mark.asyncio
    async def test_list_areas_field_types(self, rust_ws_client) -> None:
        """Test that area entry fields have correct types."""
        response = await rust_ws_client.call("config/area_registry/list")

        for entry in response["result"]:
            # Required string fields
            assert isinstance(entry["area_id"], str)
            assert isinstance(entry["name"], str)

            # Array fields
            assert isinstance(entry["aliases"], list)
            assert isinstance(entry["labels"], list)

            # Nullable string fields
            for field in ["floor_id", "humidity_entity_id", "icon", "picture", "temperature_entity_id"]:
                assert entry[field] is None or isinstance(entry[field], str), \
                    f"{field} should be string or null"

            # Timestamp fields
            assert isinstance(entry["created_at"], (int, float))
            assert isinstance(entry["modified_at"], (int, float))

    @pytest.mark.skip(reason="config/area_registry/create not yet implemented")
    @pytest.mark.asyncio
    async def test_create_area(self, rust_ws_client) -> None:
        """Test creating an area."""
        response = await rust_ws_client.call(
            "config/area_registry/create",
            name="Test Area",
        )

        assert response["success"] is True
        assert response["result"]["name"] == "Test Area"

    @pytest.mark.skip(reason="config/area_registry/update not yet implemented")
    @pytest.mark.asyncio
    async def test_update_area_not_found(self, rust_ws_client) -> None:
        """Test updating a non-existent area returns error."""
        response = await rust_ws_client.call(
            "config/area_registry/update",
            area_id="nonexistent_area",
            name="New Name",
        )

        assert response["success"] is False

    @pytest.mark.skip(reason="config/area_registry/delete not yet implemented")
    @pytest.mark.asyncio
    async def test_delete_area_not_found(self, rust_ws_client) -> None:
        """Test deleting a non-existent area returns error."""
        response = await rust_ws_client.call(
            "config/area_registry/delete",
            area_id="nonexistent_area",
        )

        assert response["success"] is False


class TestFloorRegistry:
    """Tests for floor registry WebSocket commands."""

    @pytest.mark.asyncio
    async def test_list_floors(self, rust_ws_client) -> None:
        """Test listing floors."""
        response = await rust_ws_client.call("config/floor_registry/list")

        assert response["success"] is True
        assert response["type"] == "result"
        assert isinstance(response["result"], list)

    @pytest.mark.asyncio
    async def test_list_floors_entry_fields(self, rust_ws_client) -> None:
        """Test each floor entry has required fields."""
        response = await rust_ws_client.call("config/floor_registry/list")

        required_fields = [
            "aliases",
            "created_at",
            "floor_id",
            "icon",
            "level",
            "modified_at",
            "name",
        ]

        for entry in response["result"]:
            for field in required_fields:
                assert field in entry, f"Missing field: {field}"

    @pytest.mark.asyncio
    async def test_list_floors_field_types(self, rust_ws_client) -> None:
        """Test that floor entry fields have correct types."""
        response = await rust_ws_client.call("config/floor_registry/list")

        for entry in response["result"]:
            # Required string fields
            assert isinstance(entry["floor_id"], str)
            assert isinstance(entry["name"], str)

            # Array fields
            assert isinstance(entry["aliases"], list)

            # Nullable fields
            assert entry["icon"] is None or isinstance(entry["icon"], str)
            assert entry["level"] is None or isinstance(entry["level"], int)

            # Timestamp fields
            assert isinstance(entry["created_at"], (int, float))
            assert isinstance(entry["modified_at"], (int, float))

    @pytest.mark.skip(reason="config/floor_registry/create not yet implemented")
    @pytest.mark.asyncio
    async def test_create_floor(self, rust_ws_client) -> None:
        """Test creating a floor."""
        response = await rust_ws_client.call(
            "config/floor_registry/create",
            name="Test Floor",
            level=1,
        )

        assert response["success"] is True
        assert response["result"]["name"] == "Test Floor"


class TestLabelRegistry:
    """Tests for label registry WebSocket commands."""

    @pytest.mark.asyncio
    async def test_list_labels(self, rust_ws_client) -> None:
        """Test listing labels."""
        response = await rust_ws_client.call("config/label_registry/list")

        assert response["success"] is True
        assert response["type"] == "result"
        assert isinstance(response["result"], list)

    @pytest.mark.asyncio
    async def test_list_labels_entry_fields(self, rust_ws_client) -> None:
        """Test each label entry has required fields."""
        response = await rust_ws_client.call("config/label_registry/list")

        required_fields = [
            "color",
            "created_at",
            "description",
            "icon",
            "label_id",
            "modified_at",
            "name",
        ]

        for entry in response["result"]:
            for field in required_fields:
                assert field in entry, f"Missing field: {field}"

    @pytest.mark.asyncio
    async def test_list_labels_field_types(self, rust_ws_client) -> None:
        """Test that label entry fields have correct types."""
        response = await rust_ws_client.call("config/label_registry/list")

        for entry in response["result"]:
            # Required string fields
            assert isinstance(entry["label_id"], str)
            assert isinstance(entry["name"], str)

            # Nullable string fields
            for field in ["color", "description", "icon"]:
                assert entry[field] is None or isinstance(entry[field], str), \
                    f"{field} should be string or null"

            # Timestamp fields
            assert isinstance(entry["created_at"], (int, float))
            assert isinstance(entry["modified_at"], (int, float))

    @pytest.mark.skip(reason="config/label_registry/create not yet implemented")
    @pytest.mark.asyncio
    async def test_create_label(self, rust_ws_client) -> None:
        """Test creating a label."""
        response = await rust_ws_client.call(
            "config/label_registry/create",
            name="Test Label",
            color="red",
        )

        assert response["success"] is True
        assert response["result"]["name"] == "Test Label"


class TestRegistryConsistency:
    """Tests for registry consistency across multiple calls."""

    @pytest.mark.asyncio
    async def test_all_registries_accessible(self, rust_ws_client) -> None:
        """Test that all registry list commands succeed."""
        registries = [
            "config/device_registry/list",
            "config/entity_registry/list",
            "config/area_registry/list",
            "config/floor_registry/list",
            "config/label_registry/list",
        ]

        for registry in registries:
            response = await rust_ws_client.call(registry)
            assert response["success"] is True, f"Failed to list {registry}"
            assert isinstance(response["result"], list), f"{registry} result should be list"

    @pytest.mark.asyncio
    async def test_registries_return_consistent_data(self, rust_ws_client) -> None:
        """Test that repeated calls to registries return consistent data."""
        registries = [
            "config/device_registry/list",
            "config/entity_registry/list",
            "config/area_registry/list",
            "config/floor_registry/list",
            "config/label_registry/list",
        ]

        for registry in registries:
            response1 = await rust_ws_client.call(registry)
            response2 = await rust_ws_client.call(registry)

            assert response1["result"] == response2["result"], \
                f"{registry} returned inconsistent data"
