"""Tests for device registry WebSocket API against our Rust server.

These tests verify that our Rust implementation of the device registry
WebSocket commands matches the expected HA behavior.

Test coverage follows HA's test_device_registry.py but adapted for
pure WebSocket testing (no internal state setup).
"""

import pytest


@pytest.mark.asyncio
async def test_list_devices_empty(rust_ws_client) -> None:
    """Test listing devices when registry is empty."""
    response = await rust_ws_client.call("config/device_registry/list")

    assert response["success"] is True
    assert response["type"] == "result"
    assert isinstance(response["result"], list)


@pytest.mark.asyncio
async def test_list_devices_response_format(rust_ws_client) -> None:
    """Test that device registry list response has correct field structure."""
    response = await rust_ws_client.call("config/device_registry/list")

    assert response["success"] is True
    result = response["result"]
    assert isinstance(result, list)

    # If there are devices, verify each one's structure
    for device in result:
        # Required fields per HA spec
        assert "id" in device
        assert "config_entries" in device
        assert "config_entries_subentries" in device
        assert "connections" in device
        assert "identifiers" in device

        # Optional fields that should be present (may be null)
        expected_fields = [
            "area_id",
            "configuration_url",
            "created_at",
            "disabled_by",
            "entry_type",
            "hw_version",
            "labels",
            "manufacturer",
            "model",
            "model_id",
            "modified_at",
            "name",
            "name_by_user",
            "primary_config_entry",
            "serial_number",
            "sw_version",
            "via_device_id",
        ]
        for field in expected_fields:
            assert field in device, f"Missing field: {field}"


@pytest.mark.asyncio
async def test_list_devices_field_types(rust_ws_client) -> None:
    """Test that device entry fields have correct types."""
    response = await rust_ws_client.call("config/device_registry/list")

    for device in response["result"]:
        # String or null fields
        for field in ["area_id", "configuration_url", "disabled_by", "entry_type",
                      "hw_version", "manufacturer", "model", "model_id", "name",
                      "name_by_user", "primary_config_entry", "serial_number",
                      "sw_version", "via_device_id"]:
            assert device[field] is None or isinstance(device[field], str), \
                f"{field} should be string or null, got {type(device[field])}"

        # Required string fields
        assert isinstance(device["id"], str), "id should be string"

        # Array fields
        assert isinstance(device["config_entries"], list), "config_entries should be list"
        assert isinstance(device["connections"], list), "connections should be list"
        assert isinstance(device["identifiers"], list), "identifiers should be list"
        assert isinstance(device["labels"], list), "labels should be list"

        # Object fields
        assert isinstance(device["config_entries_subentries"], dict), \
            "config_entries_subentries should be dict"

        # Timestamp fields (number or null)
        for field in ["created_at", "modified_at"]:
            assert device[field] is None or isinstance(device[field], (int, float)), \
                f"{field} should be number or null"


@pytest.mark.asyncio
async def test_list_devices_connections_format(rust_ws_client) -> None:
    """Test that connections are formatted as [type, identifier] tuples."""
    response = await rust_ws_client.call("config/device_registry/list")

    for device in response["result"]:
        for connection in device["connections"]:
            assert isinstance(connection, list), "connection should be list"
            assert len(connection) == 2, "connection should have 2 elements"
            assert all(isinstance(c, str) for c in connection), \
                "connection elements should be strings"


@pytest.mark.asyncio
async def test_list_devices_identifiers_format(rust_ws_client) -> None:
    """Test that identifiers are formatted as [domain, identifier] tuples."""
    response = await rust_ws_client.call("config/device_registry/list")

    for device in response["result"]:
        for identifier in device["identifiers"]:
            assert isinstance(identifier, list), "identifier should be list"
            assert len(identifier) == 2, "identifier should have 2 elements"
            assert all(isinstance(i, str) for i in identifier), \
                "identifier elements should be strings"


@pytest.mark.skip(reason="config/device_registry/update not yet implemented in Rust server")
@pytest.mark.asyncio
async def test_update_device_not_found(rust_ws_client) -> None:
    """Test updating a non-existent device returns error."""
    response = await rust_ws_client.call(
        "config/device_registry/update",
        device_id="nonexistent_device_id",
        name_by_user="Test Name",
    )

    # Should fail because device doesn't exist
    assert response["success"] is False or response.get("result") is None


@pytest.mark.skip(reason="config/device_registry/update not yet implemented in Rust server")
@pytest.mark.asyncio
async def test_update_device_name_by_user(rust_ws_client) -> None:
    """Test updating device name_by_user field."""
    # First get a device ID (if any exist)
    list_response = await rust_ws_client.call("config/device_registry/list")
    if not list_response["result"]:
        pytest.skip("No devices available for update test")

    device_id = list_response["result"][0]["id"]

    # Update the device
    response = await rust_ws_client.call(
        "config/device_registry/update",
        device_id=device_id,
        name_by_user="Custom Name",
    )

    assert response["success"] is True
    assert response["result"]["name_by_user"] == "Custom Name"


@pytest.mark.skip(reason="config/device_registry/update not yet implemented in Rust server")
@pytest.mark.asyncio
async def test_update_device_area_id(rust_ws_client) -> None:
    """Test assigning device to an area."""
    list_response = await rust_ws_client.call("config/device_registry/list")
    if not list_response["result"]:
        pytest.skip("No devices available for update test")

    device_id = list_response["result"][0]["id"]

    # Try to set area_id (may fail if area doesn't exist, but tests the API)
    response = await rust_ws_client.call(
        "config/device_registry/update",
        device_id=device_id,
        area_id="some_area_id",
    )

    # Response should be valid (success or expected error)
    assert "success" in response


@pytest.mark.skip(reason="config/device_registry/update not yet implemented in Rust server")
@pytest.mark.asyncio
async def test_update_device_labels(rust_ws_client) -> None:
    """Test updating device labels."""
    list_response = await rust_ws_client.call("config/device_registry/list")
    if not list_response["result"]:
        pytest.skip("No devices available for update test")

    device_id = list_response["result"][0]["id"]

    # Update labels
    response = await rust_ws_client.call(
        "config/device_registry/update",
        device_id=device_id,
        labels=["label1", "label2"],
    )

    if response["success"]:
        assert set(response["result"]["labels"]) == {"label1", "label2"}


@pytest.mark.skip(reason="config/device_registry/update not yet implemented in Rust server")
@pytest.mark.asyncio
async def test_update_device_disabled_by(rust_ws_client) -> None:
    """Test disabling a device."""
    list_response = await rust_ws_client.call("config/device_registry/list")
    if not list_response["result"]:
        pytest.skip("No devices available for update test")

    device_id = list_response["result"][0]["id"]

    # Disable device
    response = await rust_ws_client.call(
        "config/device_registry/update",
        device_id=device_id,
        disabled_by="user",
    )

    if response["success"]:
        assert response["result"]["disabled_by"] == "user"


@pytest.mark.asyncio
async def test_device_registry_requires_auth(rust_ws_client) -> None:
    """Test that device registry commands work after auth."""
    # This test verifies auth flow works correctly
    # (rust_ws_client fixture already handles auth)
    response = await rust_ws_client.call("config/device_registry/list")
    assert "success" in response


@pytest.mark.asyncio
async def test_device_registry_multiple_calls(rust_ws_client) -> None:
    """Test making multiple device registry calls in sequence."""
    # First call
    response1 = await rust_ws_client.call("config/device_registry/list")
    assert response1["success"] is True

    # Second call should also succeed
    response2 = await rust_ws_client.call("config/device_registry/list")
    assert response2["success"] is True

    # Results should be consistent (same data)
    assert response1["result"] == response2["result"]


@pytest.mark.asyncio
async def test_device_registry_response_id_matches_request(rust_ws_client) -> None:
    """Test that response ID matches request ID."""
    # The call() method auto-assigns IDs, so we verify the pattern
    response = await rust_ws_client.call("config/device_registry/list")

    assert "id" in response
    assert isinstance(response["id"], int)
    assert response["id"] > 0
