"""Home Assistant shim layer.

This package provides the homeassistant.* namespace that Python integrations expect,
backed by Rust implementations via PyO3.

Strategy:
- Shim modules we've created (core.py, const.py, etc.) take precedence
- Unknown modules fall back to native HA via extended __path__
- Entity base classes inherit from native HA with Rust state routing
- Core types (HomeAssistant, EventBus, etc.) are Rust-backed proxies

The _native_loader is used to explicitly load native modules for re-export
(e.g., constants, types we inherit from).
"""

import logging
from pathlib import Path

_LOGGER = logging.getLogger(__name__)


# Always extend __path__ to include native HA's package directory.
# This allows Python integrations to import modules we haven't shimmed yet.
# Shim modules (core.py, const.py, etc.) take precedence because they're
# direct files in this directory.
def _find_native_ha():
    current = Path(__file__).resolve().parent
    for _ in range(10):
        vendor_path = current / "vendor" / "ha-core" / "homeassistant"
        if vendor_path.exists():
            return vendor_path
        parent = current.parent
        if parent == current:
            break
        current = parent
    # Fallback: try from cwd
    cwd_vendor = Path.cwd() / "vendor" / "ha-core" / "homeassistant"
    if cwd_vendor.exists():
        return cwd_vendor
    return None


_native_ha = _find_native_ha()
if _native_ha:
    __path__.append(str(_native_ha))

from homeassistant.core import HomeAssistant, callback

__all__ = ["HomeAssistant", "callback"]
