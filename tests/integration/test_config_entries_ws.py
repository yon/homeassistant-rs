"""Tests for config entries WebSocket API against our Rust server.

These tests verify that our Rust implementation of the config entries
WebSocket commands matches the expected HA behavior.

Test coverage follows HA's test_config_entries.py but adapted for
pure WebSocket testing (no internal state setup).
"""

import pytest


class TestConfigEntriesGet:
    """Tests for config_entries/get command."""

    @pytest.mark.asyncio
    async def test_get_config_entries(self, rust_ws_client) -> None:
        """Test getting config entries."""
        response = await rust_ws_client.call("config_entries/get")

        assert response["success"] is True
        assert isinstance(response["result"], list)

    @pytest.mark.asyncio
    async def test_get_config_entries_entry_fields(self, rust_ws_client) -> None:
        """Test each config entry has all required fields."""
        response = await rust_ws_client.call("config_entries/get")

        required_fields = [
            "entry_id",
            "domain",
            "title",
            "source",
            "state",
            "supports_options",
            "supports_remove_device",
            "supports_unload",
            "supports_reconfigure",
            "pref_disable_new_entities",
            "pref_disable_polling",
            "disabled_by",
            "reason",
            "error_reason_translation_key",
            "error_reason_translation_placeholders",
        ]

        for entry in response["result"]:
            for field in required_fields:
                assert field in entry, f"Missing field: {field}"

    @pytest.mark.asyncio
    async def test_get_config_entries_valid_states(self, rust_ws_client) -> None:
        """Test that config entry states are valid."""
        response = await rust_ws_client.call("config_entries/get")

        valid_states = {
            "not_loaded",
            "setup_in_progress",
            "loaded",
            "setup_error",
            "setup_retry",
            "migration_error",
            "failed_unload",
        }

        for entry in response["result"]:
            assert entry["state"] in valid_states, \
                f"Invalid state '{entry['state']}' for entry {entry['entry_id']}"

    @pytest.mark.asyncio
    async def test_get_config_entries_by_domain(self, rust_ws_client) -> None:
        """Test filtering config entries by domain."""
        response = await rust_ws_client.call(
            "config_entries/get",
            domain="nonexistent_domain",
        )

        assert response["success"] is True
        assert isinstance(response["result"], list)
        # No entries for non-existent domain
        assert response["result"] == []

    @pytest.mark.asyncio
    async def test_get_config_entries_by_entry_id(self, rust_ws_client) -> None:
        """Test getting a specific config entry by entry_id."""
        response = await rust_ws_client.call(
            "config_entries/get",
            entry_id="nonexistent_entry",
        )

        assert response["success"] is True
        # Result can be None or an entry object depending on implementation
        result = response["result"]
        if result is not None and isinstance(result, dict):
            assert result.get("entry_id") == "nonexistent_entry"

    @pytest.mark.asyncio
    async def test_get_config_entries_field_types(self, rust_ws_client) -> None:
        """Test that config entry fields have correct types."""
        response = await rust_ws_client.call("config_entries/get")

        for entry in response["result"]:
            # Required string fields
            assert isinstance(entry["entry_id"], str)
            assert isinstance(entry["domain"], str)
            assert isinstance(entry["title"], str)
            assert isinstance(entry["source"], str)
            assert isinstance(entry["state"], str)

            # Boolean fields
            assert isinstance(entry["supports_options"], bool)
            assert isinstance(entry["supports_remove_device"], bool)
            assert isinstance(entry["supports_unload"], bool)
            assert isinstance(entry["supports_reconfigure"], bool)
            assert isinstance(entry["pref_disable_new_entities"], bool)
            assert isinstance(entry["pref_disable_polling"], bool)

            # Nullable fields
            assert entry["disabled_by"] is None or isinstance(entry["disabled_by"], str)
            assert entry["reason"] is None or isinstance(entry["reason"], str)
            assert entry["error_reason_translation_key"] is None or isinstance(entry["error_reason_translation_key"], str)
            assert entry["error_reason_translation_placeholders"] is None or isinstance(entry["error_reason_translation_placeholders"], dict)


class TestConfigEntriesSubscribe:
    """Tests for config_entries/subscribe command."""

    @pytest.mark.asyncio
    async def test_subscribe_config_entries(self, rust_ws_client) -> None:
        """Test subscribing to config entries updates."""
        response = await rust_ws_client.call("config_entries/subscribe")

        assert response["success"] is True

    @pytest.mark.skip(reason="type_filter parameter not yet supported")
    @pytest.mark.asyncio
    async def test_subscribe_with_type_filter(self, rust_ws_client) -> None:
        """Test subscribing with type filter."""
        # Test with various type filters
        for type_filter in [None, "device", "service", "hub"]:
            if type_filter:
                response = await rust_ws_client.call(
                    "config_entries/subscribe",
                    type_filter=type_filter,
                )
            else:
                response = await rust_ws_client.call("config_entries/subscribe")

            assert response["success"] is True


class TestConfigEntriesSubentries:
    """Tests for config_entries/subentries commands."""

    @pytest.mark.asyncio
    async def test_subentries_list_nonexistent_entry(self, rust_ws_client) -> None:
        """Test listing subentries for a non-existent config entry."""
        response = await rust_ws_client.call(
            "config_entries/subentries/list",
            entry_id="nonexistent_entry_id",
        )

        # Our implementation returns empty array for non-existent entries
        # (matching HA's behavior of not exposing entry existence)
        assert response["success"] is True
        assert response["result"] == []

    @pytest.mark.asyncio
    async def test_subentries_list_empty(self, rust_ws_client) -> None:
        """Test listing subentries returns empty array when none exist."""
        response = await rust_ws_client.call(
            "config_entries/subentries/list",
            entry_id="test_entry",
        )

        assert response["success"] is True
        assert isinstance(response["result"], list)
        # For a new/empty entry, should return empty array
        assert response["result"] == []

    @pytest.mark.asyncio
    async def test_subentries_list_response_format(self, rust_ws_client) -> None:
        """Test subentries list response format."""
        # Get actual entries first
        entries_response = await rust_ws_client.call("config_entries/get")

        if entries_response["result"]:
            entry_id = entries_response["result"][0]["entry_id"]

            response = await rust_ws_client.call(
                "config_entries/subentries/list",
                entry_id=entry_id,
            )

            assert response["success"] is True
            assert isinstance(response["result"], list)


class TestConfigEntriesFlow:
    """Tests for config_entries/flow commands."""

    @pytest.mark.skip(reason="config_entries/flow/subscribe not yet implemented")
    @pytest.mark.asyncio
    async def test_flow_subscribe(self, rust_ws_client) -> None:
        """Test subscribing to config flow updates."""
        response = await rust_ws_client.call("config_entries/flow/subscribe")

        assert response["success"] is True


class TestConfigEntriesMultipleCalls:
    """Tests for making multiple config entry calls."""

    @pytest.mark.asyncio
    async def test_multiple_get_calls(self, rust_ws_client) -> None:
        """Test making multiple get calls returns consistent data."""
        response1 = await rust_ws_client.call("config_entries/get")
        response2 = await rust_ws_client.call("config_entries/get")

        assert response1["success"] is True
        assert response2["success"] is True
        assert response1["result"] == response2["result"]

    @pytest.mark.asyncio
    async def test_get_and_subscribe_sequence(self, rust_ws_client) -> None:
        """Test getting entries then subscribing."""
        # Get entries
        get_response = await rust_ws_client.call("config_entries/get")
        assert get_response["success"] is True

        # Then subscribe
        sub_response = await rust_ws_client.call("config_entries/subscribe")
        assert sub_response["success"] is True

    @pytest.mark.asyncio
    async def test_response_ids_increment(self, rust_ws_client) -> None:
        """Test that response IDs match request IDs and increment."""
        response1 = await rust_ws_client.call("config_entries/get")
        response2 = await rust_ws_client.call("config_entries/get")

        assert response1["id"] < response2["id"]
