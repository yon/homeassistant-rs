"""Home Assistant shim layer.

This package provides the homeassistant.* namespace that Python integrations expect,
backed by Rust implementations via PyO3.

Strategy:
- Re-export safe modules (constants, types, exceptions) from native HA
- Inherit entity base classes from native HA, override methods that route to Rust
- Implement core types (HomeAssistant, EventBus, etc.) as Rust-backed proxies
- STRICT by default: unknown modules raise ImportError (ensures we know what's ported)
- Set ALLOW_HA_NATIVE_FALLBACK=1 for development to fall back to native HA

The _native_loader is ONLY for loading native modules that we explicitly re-export
(constants, base classes we inherit from). It is NOT a general fallback.
"""

import logging
import os
from pathlib import Path

_LOGGER = logging.getLogger(__name__)

# Check if native fallback is allowed (development mode only)
_ALLOW_HA_NATIVE_FALLBACK = os.environ.get("ALLOW_HA_NATIVE_FALLBACK", "").lower() in ("1", "true", "yes")

if _ALLOW_HA_NATIVE_FALLBACK:
    _LOGGER.warning(
        "ALLOW_HA_NATIVE_FALLBACK is enabled - unknown modules will fall back to native HA. "
        "This should only be used for development."
    )
    # Extend __path__ to include native HA's package directory
    # Search upward from this file to find vendor/ha-core
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
