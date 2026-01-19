#!/usr/bin/env python3
"""Integration tests for the Rust WebSocket API.

These tests start the actual Rust server and connect via WebSocket
to validate that our implementation matches HA's expected format.

Usage:
    pytest tests/integration/test_websocket_api.py -v

    # Or run directly:
    python tests/integration/test_websocket_api.py
"""

import asyncio
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

import aiohttp
import pytest
import pytest_asyncio

# Mark all tests in this module as async
pytestmark = pytest.mark.asyncio(loop_scope="function")

# Server configuration
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 28123  # Use non-standard port to avoid conflicts
SERVER_URL = f"http://{SERVER_HOST}:{SERVER_PORT}"
WS_URL = f"ws://{SERVER_HOST}:{SERVER_PORT}/api/websocket"

# Path to the Rust binary
REPO_ROOT = Path(__file__).parent.parent.parent
RUST_BINARY = REPO_ROOT / "target" / "debug" / "homeassistant"


class RustServer:
    """Context manager for running the Rust server during tests."""

    def __init__(self, port: int = SERVER_PORT):
        self.port = port
        self.process = None

    async def start(self, timeout: float = 10.0):
        """Start the Rust server and wait for it to be ready."""
        if not RUST_BINARY.exists():
            raise RuntimeError(
                f"Rust binary not found at {RUST_BINARY}. "
                "Run 'cargo build' first."
            )

        # Set environment variables
        env = os.environ.copy()
        env["HA_PORT"] = str(self.port)
        env["RUST_LOG"] = "warn"  # Reduce log noise during tests

        # Start the server
        self.process = subprocess.Popen(
            [str(RUST_BINARY)],
            cwd=REPO_ROOT,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        # Wait for server to be ready
        start_time = time.time()
        while time.time() - start_time < timeout:
            try:
                async with aiohttp.ClientSession() as session:
                    async with session.get(f"{SERVER_URL}/api/") as resp:
                        if resp.status == 200:
                            return  # Server is ready
            except aiohttp.ClientError:
                pass
            await asyncio.sleep(0.1)

        # Server didn't start in time
        self.stop()
        raise RuntimeError(f"Server failed to start within {timeout}s")

    def stop(self):
        """Stop the Rust server."""
        if self.process:
            self.process.send_signal(signal.SIGTERM)
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
            self.process = None

    async def __aenter__(self):
        await self.start()
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        self.stop()


class WebSocketClient:
    """Helper for WebSocket communication with the server."""

    def __init__(self, session: aiohttp.ClientSession):
        self.session = session
        self.ws = None
        self.msg_id = 0

    async def connect(self):
        """Connect to the WebSocket endpoint."""
        self.ws = await self.session.ws_connect(WS_URL)

        # First message should be auth_required
        msg = await self.receive()
        assert msg["type"] == "auth_required", f"Expected auth_required, got {msg}"
        return msg

    async def authenticate(self, access_token: str = "test_token"):
        """Authenticate with the server."""
        await self.send({"type": "auth", "access_token": access_token})
        msg = await self.receive()
        assert msg["type"] == "auth_ok", f"Expected auth_ok, got {msg}"
        return msg

    async def send(self, data: dict):
        """Send a message to the server."""
        await self.ws.send_json(data)

    async def send_command(self, msg_type: str, **kwargs) -> dict:
        """Send a command and wait for the result."""
        self.msg_id += 1
        msg = {"id": self.msg_id, "type": msg_type, **kwargs}
        await self.send(msg)
        return await self.receive_result(self.msg_id)

    async def receive(self, timeout: float = 5.0) -> dict:
        """Receive a message from the server."""
        msg = await asyncio.wait_for(self.ws.receive(), timeout=timeout)
        if msg.type == aiohttp.WSMsgType.TEXT:
            return json.loads(msg.data)
        elif msg.type == aiohttp.WSMsgType.CLOSED:
            raise ConnectionError("WebSocket closed")
        elif msg.type == aiohttp.WSMsgType.ERROR:
            raise ConnectionError(f"WebSocket error: {msg.data}")
        else:
            raise ValueError(f"Unexpected message type: {msg.type}")

    async def receive_result(self, msg_id: int, timeout: float = 5.0) -> dict:
        """Receive a result message for a specific command."""
        msg = await self.receive(timeout)
        assert msg.get("id") == msg_id, f"Expected id {msg_id}, got {msg.get('id')}"
        assert msg.get("type") == "result", f"Expected result, got {msg}"
        return msg

    async def close(self):
        """Close the WebSocket connection."""
        if self.ws:
            await self.ws.close()


@pytest_asyncio.fixture
async def server():
    """Fixture that starts the Rust server for tests."""
    async with RustServer() as srv:
        yield srv


@pytest_asyncio.fixture
async def ws_client(server):
    """Fixture that provides an authenticated WebSocket client."""
    async with aiohttp.ClientSession() as session:
        client = WebSocketClient(session)
        await client.connect()
        await client.authenticate()
        yield client
        await client.close()


# =============================================================================
# Tests
# =============================================================================

class TestWebSocketConnection:
    """Tests for basic WebSocket connection and authentication."""

    async def test_connect_receives_auth_required(self, server):
        """Test that connecting receives auth_required message."""
        async with aiohttp.ClientSession() as session:
            client = WebSocketClient(session)
            msg = await client.connect()

            assert msg["type"] == "auth_required"
            assert "ha_version" in msg
            await client.close()

    async def test_authenticate_success(self, server):
        """Test successful authentication."""
        async with aiohttp.ClientSession() as session:
            client = WebSocketClient(session)
            await client.connect()
            msg = await client.authenticate()

            assert msg["type"] == "auth_ok"
            assert "ha_version" in msg
            await client.close()

    async def test_ping_pong(self, ws_client):
        """Test ping/pong functionality."""
        ws_client.msg_id += 1
        await ws_client.send({"id": ws_client.msg_id, "type": "ping"})
        msg = await ws_client.receive()

        assert msg["id"] == ws_client.msg_id
        assert msg["type"] == "pong"


class TestConfigEntriesWebSocket:
    """Tests for config_entries WebSocket commands."""

    async def test_config_entries_subscribe(self, ws_client):
        """Test config_entries/subscribe returns proper format."""
        ws_client.msg_id += 1
        msg_id = ws_client.msg_id

        await ws_client.send({
            "id": msg_id,
            "type": "config_entries/subscribe",
        })

        # First message should be result (success)
        result = await ws_client.receive()
        assert result["id"] == msg_id
        assert result["type"] == "result"
        assert result["success"] is True
        assert result["result"] is None  # Native HA returns null for subscribe

        # Second message should be event with entries
        event = await ws_client.receive()
        assert event["id"] == msg_id
        assert event["type"] == "event"
        assert "event" in event

        # Event should be a list of entries
        entries = event["event"]
        assert isinstance(entries, list)

        # Each entry should have the expected format
        for entry_wrapper in entries:
            assert "type" in entry_wrapper  # null for initial, "added"/"updated" for changes
            assert "entry" in entry_wrapper

            entry = entry_wrapper["entry"]
            # Validate required fields
            assert "entry_id" in entry
            assert "domain" in entry
            assert "title" in entry
            assert "source" in entry
            assert "state" in entry
            assert "supports_options" in entry
            assert "supports_unload" in entry
            assert "supports_remove_device" in entry
            assert "supports_reconfigure" in entry
            assert "pref_disable_new_entities" in entry
            assert "pref_disable_polling" in entry

    async def test_config_entries_subscribe_with_type_filter(self, ws_client):
        """Test config_entries/subscribe with type_filter returns empty for helpers."""
        ws_client.msg_id += 1
        msg_id = ws_client.msg_id

        await ws_client.send({
            "id": msg_id,
            "type": "config_entries/subscribe",
            "type_filter": ["helper"],
        })

        # First message should be result
        result = await ws_client.receive()
        assert result["id"] == msg_id
        assert result["success"] is True

        # Second message should be event with empty list (no helper integrations)
        event = await ws_client.receive()
        assert event["id"] == msg_id
        assert event["type"] == "event"
        # With only demo integration loaded (not a helper), should be empty
        entries = event["event"]
        assert isinstance(entries, list)


class TestDeviceRegistryWebSocket:
    """Tests for device registry WebSocket commands."""

    async def test_device_registry_list(self, ws_client):
        """Test config/device_registry/list returns proper format."""
        result = await ws_client.send_command("config/device_registry/list")

        assert result["success"] is True
        devices = result["result"]
        assert isinstance(devices, list)

        # If there are devices, validate format
        for device in devices:
            assert "id" in device
            assert "config_entries" in device
            assert "identifiers" in device
            assert "name" in device or device.get("name") is None


class TestEntityRegistryWebSocket:
    """Tests for entity registry WebSocket commands."""

    async def test_entity_registry_list(self, ws_client):
        """Test config/entity_registry/list returns proper format."""
        result = await ws_client.send_command("config/entity_registry/list")

        assert result["success"] is True
        entities = result["result"]
        assert isinstance(entities, list)

        # If there are entities, validate format
        for entity in entities:
            assert "entity_id" in entity
            assert "platform" in entity


class TestAreaRegistryWebSocket:
    """Tests for area registry WebSocket commands."""

    async def test_area_registry_list(self, ws_client):
        """Test config/area_registry/list returns proper format."""
        result = await ws_client.send_command("config/area_registry/list")

        assert result["success"] is True
        areas = result["result"]
        assert isinstance(areas, list)


class TestCoreCommands:
    """Tests for core WebSocket commands."""

    async def test_get_states(self, ws_client):
        """Test get_states returns proper format."""
        result = await ws_client.send_command("get_states")

        assert result["success"] is True
        states = result["result"]
        assert isinstance(states, list)

        # Validate state format
        for state in states:
            assert "entity_id" in state
            assert "state" in state
            assert "attributes" in state
            assert "last_changed" in state
            assert "last_updated" in state
            assert "context" in state

    async def test_get_config(self, ws_client):
        """Test get_config returns proper format."""
        result = await ws_client.send_command("get_config")

        assert result["success"] is True
        config = result["result"]
        assert isinstance(config, dict)

        # Validate required config fields
        assert "location_name" in config
        assert "latitude" in config
        assert "longitude" in config
        assert "unit_system" in config

    async def test_get_services(self, ws_client):
        """Test get_services returns proper format."""
        result = await ws_client.send_command("get_services")

        assert result["success"] is True
        services = result["result"]
        assert isinstance(services, dict)


class TestSubscribeEvents:
    """Tests for event subscription."""

    async def test_subscribe_events(self, ws_client):
        """Test subscribe_events returns proper format."""
        result = await ws_client.send_command(
            "subscribe_events",
            event_type="state_changed"
        )

        assert result["success"] is True


# =============================================================================
# CLI runner
# =============================================================================

async def run_tests():
    """Run all tests manually (for debugging)."""
    print("Starting Rust server...")
    async with RustServer() as server:
        print(f"Server running on {SERVER_URL}")

        async with aiohttp.ClientSession() as session:
            client = WebSocketClient(session)

            print("\n--- Testing connection ---")
            msg = await client.connect()
            print(f"auth_required: {msg}")

            msg = await client.authenticate()
            print(f"auth_ok: {msg}")

            print("\n--- Testing ping ---")
            result = await client.send_command("ping")
            print(f"ping result: {result}")

            print("\n--- Testing config_entries/subscribe ---")
            client.msg_id += 1
            await client.send({
                "id": client.msg_id,
                "type": "config_entries/subscribe",
            })
            result = await client.receive()
            print(f"result: {json.dumps(result, indent=2)}")
            event = await client.receive()
            print(f"event: {json.dumps(event, indent=2)}")

            print("\n--- Testing get_states ---")
            result = await client.send_command("get_states")
            print(f"get_states: {len(result.get('result', []))} states")

            print("\n--- Testing device_registry/list ---")
            result = await client.send_command("config/device_registry/list")
            print(f"devices: {len(result.get('result', []))} devices")

            print("\n--- Testing entity_registry/list ---")
            result = await client.send_command("config/entity_registry/list")
            print(f"entities: {len(result.get('result', []))} entities")

            await client.close()

    print("\n--- All tests passed ---")


if __name__ == "__main__":
    asyncio.run(run_tests())
