"""Tests for application_credentials WebSocket API against our Rust server.

These tests verify that our Rust implementation of the application_credentials
WebSocket commands matches the expected HA behavior.

Test coverage follows HA's tests/components/application_credentials/test_init.py
but adapted for pure WebSocket testing (no Python HA internals).
"""

import pytest

# Constants from HA tests
CLIENT_ID = "some-client-id"
CLIENT_SECRET = "some-client-secret"
TEST_DOMAIN = "fake_integration"


class TestApplicationCredentialsList:
    """Tests for application_credentials/list command."""

    @pytest.mark.asyncio
    async def test_list_empty(self, rust_ws_client) -> None:
        """Test listing credentials when none exist."""
        response = await rust_ws_client.call("application_credentials/list")

        assert response["success"] is True
        # HA returns [] directly, we currently return it as-is
        result = response["result"]
        assert isinstance(result, list) or (isinstance(result, dict) and "credentials" in result)


class TestApplicationCredentialsConfig:
    """Tests for application_credentials/config command."""

    @pytest.mark.asyncio
    async def test_config_returns_domains(self, rust_ws_client) -> None:
        """Test config returns list of OAuth2 domains."""
        response = await rust_ws_client.call("application_credentials/config")

        assert response["success"] is True
        assert "domains" in response["result"]
        assert isinstance(response["result"]["domains"], list)


class TestApplicationCredentialsCreate:
    """Tests for application_credentials/create command."""

    @pytest.mark.asyncio
    async def test_create_credential(self, rust_ws_client) -> None:
        """Test creating a credential."""
        response = await rust_ws_client.call(
            "application_credentials/create",
            domain=TEST_DOMAIN,
            client_id=CLIENT_ID,
            client_secret=CLIENT_SECRET,
        )

        assert response["success"] is True
        result = response["result"]
        assert result["domain"] == TEST_DOMAIN
        assert result["client_id"] == CLIENT_ID
        assert result["client_secret"] == CLIENT_SECRET
        assert "id" in result

    @pytest.mark.asyncio
    async def test_create_credential_with_name(self, rust_ws_client) -> None:
        """Test creating a credential with a name."""
        response = await rust_ws_client.call(
            "application_credentials/create",
            domain=TEST_DOMAIN,
            client_id=f"{CLIENT_ID}_named",
            client_secret=CLIENT_SECRET,
            name="My Named Credential",
        )

        assert response["success"] is True
        result = response["result"]
        assert result["name"] == "My Named Credential"
        assert result["client_id"] == f"{CLIENT_ID}_named"

    @pytest.mark.asyncio
    async def test_create_strips_whitespace(self, rust_ws_client) -> None:
        """Test that create strips whitespace from credentials."""
        response = await rust_ws_client.call(
            "application_credentials/create",
            domain=TEST_DOMAIN,
            client_id=f"  {CLIENT_ID}_ws  ",
            client_secret=f" {CLIENT_SECRET} ",
        )

        assert response["success"] is True
        result = response["result"]
        # Whitespace should be stripped
        assert result["client_id"] == f"{CLIENT_ID}_ws"
        assert result["client_secret"] == CLIENT_SECRET

    @pytest.mark.asyncio
    async def test_create_then_list(self, rust_ws_client) -> None:
        """Test that created credential appears in list."""
        # Create a credential
        create_response = await rust_ws_client.call(
            "application_credentials/create",
            domain=TEST_DOMAIN,
            client_id=f"{CLIENT_ID}_list_test",
            client_secret=CLIENT_SECRET,
        )
        assert create_response["success"] is True
        created_id = create_response["result"]["id"]

        # List credentials
        list_response = await rust_ws_client.call("application_credentials/list")
        assert list_response["success"] is True

        result = list_response["result"]
        credentials = result if isinstance(result, list) else result.get("credentials", [])

        # Find our credential
        found = [c for c in credentials if c.get("id") == created_id]
        assert len(found) == 1, f"Created credential not found in list: {credentials}"


class TestApplicationCredentialsDelete:
    """Tests for application_credentials/delete command."""

    @pytest.mark.asyncio
    async def test_delete_credential(self, rust_ws_client) -> None:
        """Test deleting a credential."""
        # Create a credential
        create_response = await rust_ws_client.call(
            "application_credentials/create",
            domain=TEST_DOMAIN,
            client_id=f"{CLIENT_ID}_delete_test",
            client_secret=CLIENT_SECRET,
        )
        assert create_response["success"] is True
        created_id = create_response["result"]["id"]

        # Delete it
        delete_response = await rust_ws_client.call(
            "application_credentials/delete",
            application_credentials_id=created_id,
        )
        assert delete_response["success"] is True

        # Verify it's gone
        list_response = await rust_ws_client.call("application_credentials/list")
        result = list_response["result"]
        credentials = result if isinstance(result, list) else result.get("credentials", [])
        found = [c for c in credentials if c.get("id") == created_id]
        assert len(found) == 0, f"Credential should be deleted, found: {found}"

    @pytest.mark.asyncio
    async def test_delete_nonexistent(self, rust_ws_client) -> None:
        """Test deleting a nonexistent credential returns error."""
        response = await rust_ws_client.call(
            "application_credentials/delete",
            application_credentials_id="nonexistent_id",
        )

        assert response["success"] is False
        assert "error" in response
        assert response["error"]["code"] == "not_found"
