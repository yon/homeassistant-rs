"""Conftest that overrides HA fixtures to test against our Rust server.

This module provides replacement fixtures for HA's test infrastructure
so we can run HA's own WebSocket tests against our Rust server.

Usage:
    pytest vendor/ha-core/tests/components/config/test_device_registry.py \
        --confcutdir=tests/rust_server \
        -p tests.rust_server.conftest_ha_override
"""

import asyncio
import os
import signal
import subprocess
import time
from pathlib import Path
from typing import Any, Generator
from collections.abc import Coroutine
from unittest.mock import MagicMock, AsyncMock

import pytest
import pytest_asyncio
import aiohttp


# Configuration
RUST_SERVER_HOST = "127.0.0.1"
RUST_SERVER_PORT = 18123
RUST_SERVER_URL = f"http://{RUST_SERVER_HOST}:{RUST_SERVER_PORT}"
RUST_WS_URL = f"ws://{RUST_SERVER_HOST}:{RUST_SERVER_PORT}/api/websocket"


def get_repo_root() -> Path:
    """Get the repository root directory."""
    return Path(__file__).parent.parent.parent


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
        server_bin = repo_root / "target" / "debug" / "homeassistant"

        if not server_bin.exists():
            server_bin = repo_root / "target" / "release" / "homeassistant"

        if not server_bin.exists():
            raise RuntimeError(
                f"Rust server binary not found. Run 'cargo build -p ha-server' first.\n"
                f"Looked for: {server_bin}"
            )

        env = os.environ.copy()
        env["HA_PORT"] = str(RUST_SERVER_PORT)
        env["HA_HOST"] = RUST_SERVER_HOST
        env["RUST_LOG"] = "warn"

        if self.config_dir:
            env["HA_CONFIG_DIR"] = str(self.config_dir)

        self.process = subprocess.Popen(
            [str(server_bin)],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        start_time = time.time()
        while time.time() - start_time < timeout:
            try:
                import urllib.request
                urllib.request.urlopen(f"{RUST_SERVER_URL}/api/", timeout=1)
                self._started = True
                return
            except Exception:
                if self.process.poll() is not None:
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


# Global server instance (shared across test session)
_rust_server: RustServerProcess | None = None


@pytest.fixture(scope="session")
def rust_server(tmp_path_factory) -> Generator[RustServerProcess, None, None]:
    """Start the Rust server for the test session."""
    global _rust_server
    if _rust_server is None:
        config_dir = tmp_path_factory.mktemp("config")
        _rust_server = RustServerProcess(config_dir)
        _rust_server.start()
    yield _rust_server
    _rust_server.stop()
    _rust_server = None


@pytest.fixture
def hass(rust_server) -> MagicMock:
    """Provide a mock hass object.

    Many HA tests expect a hass object, but we're testing against
    our Rust server, so we provide a minimal mock.
    """
    mock_hass = MagicMock()
    mock_hass.data = {}
    mock_hass.states = MagicMock()
    mock_hass.services = MagicMock()
    mock_hass.config_entries = MagicMock()
    mock_hass.helpers = MagicMock()
    return mock_hass


@pytest.fixture
def hass_access_token() -> str:
    """Provide a test access token."""
    return "test_token"


@pytest.fixture
def hass_ws_client(rust_server, hass_access_token):
    """Override HA's hass_ws_client to connect to our Rust server.

    This fixture returns a factory function that creates WebSocket
    clients connected to our Rust server instead of Python HA.
    """

    async def create_client(hass=None, access_token=None):
        """Create a WebSocket client connected to the Rust server."""
        if access_token is None:
            access_token = hass_access_token

        session = aiohttp.ClientSession()
        websocket = await session.ws_connect(RUST_WS_URL)

        # Handle auth flow
        auth_resp = await websocket.receive_json()
        assert auth_resp["type"] == "auth_required", f"Expected auth_required, got {auth_resp}"

        await websocket.send_json({"type": "auth", "access_token": access_token})

        auth_ok = await websocket.receive_json()
        assert auth_ok["type"] == "auth_ok", f"Expected auth_ok, got {auth_ok}"

        # Add auto-id functionality like HA's fixture
        def _get_next_id():
            i = 0
            while True:
                i += 1
                yield i

        id_generator = _get_next_id()

        def _send_json_auto_id(data: dict) -> Coroutine[Any, Any, None]:
            data["id"] = next(id_generator)
            return websocket.send_json(data)

        # Attach extra methods to match HA's MockHAClientWebSocket
        websocket.send_json_auto_id = _send_json_auto_id
        websocket._session = session  # Keep reference for cleanup

        return websocket

    return create_client


# Override other HA fixtures that might be needed

@pytest.fixture
def socket_enabled():
    """Socket is always enabled for our tests."""
    return None


@pytest.fixture
def aiohttp_client():
    """Not used when testing against Rust server."""
    return None
