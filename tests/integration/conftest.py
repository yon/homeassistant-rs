"""Pytest configuration for testing against our Rust HA server.

This module provides fixtures for starting the Rust server and connecting
to its WebSocket API for integration testing.
"""

import asyncio
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, AsyncGenerator

import pytest
import pytest_asyncio
import aiohttp


# Configure pytest-asyncio
pytest_plugins = ('pytest_asyncio',)


def pytest_configure(config):
    """Configure pytest for async tests."""
    config.addinivalue_line(
        "markers", "asyncio: mark test as async"
    )


def get_repo_root() -> Path:
    """Get the repository root directory."""
    return Path(__file__).parent.parent.parent


# Configuration
RUST_SERVER_HOST = "127.0.0.1"
RUST_SERVER_PORT = 18123  # Use different port to avoid conflicts
RUST_SERVER_URL = f"http://{RUST_SERVER_HOST}:{RUST_SERVER_PORT}"
RUST_WS_URL = f"ws://{RUST_SERVER_HOST}:{RUST_SERVER_PORT}/api/websocket"


class RustServerProcess:
    """Manages the Rust HA server process for testing."""

    def __init__(self, config_dir: Path | None = None):
        self.process: subprocess.Popen | None = None
        self.config_dir = config_dir
        self._started = False

    def start(self, timeout: float = 30.0) -> None:
        """Start the Rust server and wait for it to be ready."""
        if self._started:
            return

        repo_root = get_repo_root()
        # The binary is named "homeassistant" per Cargo.toml [[bin]] config
        server_bin = repo_root / "target" / "debug" / "homeassistant"

        if not server_bin.exists():
            # Try release build
            server_bin = repo_root / "target" / "release" / "homeassistant"

        if not server_bin.exists():
            raise RuntimeError(
                f"Rust server binary not found. Run 'cargo build -p ha-server' first.\n"
                f"Looked for: {server_bin}"
            )

        env = os.environ.copy()
        env["HA_PORT"] = str(RUST_SERVER_PORT)
        env["HA_HOST"] = RUST_SERVER_HOST
        env["RUST_LOG"] = "warn"  # Reduce log noise during tests

        if self.config_dir:
            env["HA_CONFIG_DIR"] = str(self.config_dir)

        self.process = subprocess.Popen(
            [str(server_bin)],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        # Wait for server to be ready
        start_time = time.time()
        while time.time() - start_time < timeout:
            try:
                import urllib.request
                urllib.request.urlopen(f"{RUST_SERVER_URL}/api/", timeout=1)
                self._started = True
                return
            except Exception:
                if self.process.poll() is not None:
                    # Process died
                    stdout, stderr = self.process.communicate()
                    raise RuntimeError(
                        f"Rust server process died.\n"
                        f"stdout: {stdout.decode()}\n"
                        f"stderr: {stderr.decode()}"
                    )
                time.sleep(0.1)

        self.stop()
        raise RuntimeError(f"Rust server did not start within {timeout}s")

    def stop(self) -> None:
        """Stop the Rust server."""
        if self.process:
            self.process.send_signal(signal.SIGTERM)
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
            self.process = None
            self._started = False


class RustWebSocketClient:
    """WebSocket client for testing against our Rust server."""

    def __init__(self, session: aiohttp.ClientSession):
        self.session = session
        self.ws: aiohttp.ClientWebSocketResponse | None = None
        self._msg_id = 0

    async def connect(self) -> None:
        """Connect to the Rust server WebSocket."""
        self.ws = await self.session.ws_connect(RUST_WS_URL)

        # Wait for auth_required
        msg = await self.ws.receive_json()
        assert msg["type"] == "auth_required", f"Expected auth_required, got {msg}"

        # Send auth (our test server accepts any token)
        await self.ws.send_json({"type": "auth", "access_token": "test_token"})

        # Wait for auth_ok
        msg = await self.ws.receive_json()
        assert msg["type"] == "auth_ok", f"Expected auth_ok, got {msg}"

    async def close(self) -> None:
        """Close the WebSocket connection."""
        if self.ws:
            await self.ws.close()
            self.ws = None

    async def send_json(self, data: dict) -> None:
        """Send JSON data to the server."""
        if not self.ws:
            raise RuntimeError("Not connected")
        await self.ws.send_json(data)

    async def send_json_auto_id(self, data: dict) -> None:
        """Send JSON with auto-incremented ID."""
        self._msg_id += 1
        data["id"] = self._msg_id
        await self.send_json(data)

    async def receive_json(self, timeout: float = 10.0) -> dict:
        """Receive JSON from the server with timeout."""
        if not self.ws:
            raise RuntimeError("Not connected")
        try:
            return await asyncio.wait_for(self.ws.receive_json(), timeout=timeout)
        except asyncio.TimeoutError:
            raise TimeoutError(f"No response from server within {timeout}s")

    async def call(self, msg_type: str, **kwargs) -> dict:
        """Send a command and wait for the response."""
        self._msg_id += 1
        msg = {"type": msg_type, "id": self._msg_id, **kwargs}
        await self.send_json(msg)
        return await self.receive_json()


@pytest.fixture(scope="session")
def rust_server(tmp_path_factory) -> RustServerProcess:
    """Start the Rust server for the test session."""
    config_dir = tmp_path_factory.mktemp("config")
    server = RustServerProcess(config_dir)
    server.start()
    yield server
    server.stop()


@pytest_asyncio.fixture
async def rust_ws_client(rust_server) -> AsyncGenerator[RustWebSocketClient, None]:
    """Provide a connected WebSocket client to the Rust server."""
    async with aiohttp.ClientSession() as session:
        client = RustWebSocketClient(session)
        await client.connect()
        yield client
        await client.close()


@pytest_asyncio.fixture
async def rust_http_client(rust_server) -> AsyncGenerator[aiohttp.ClientSession, None]:
    """Provide an HTTP client session for REST API tests."""
    async with aiohttp.ClientSession(base_url=RUST_SERVER_URL) as session:
        yield session
