"""Home Assistant shim layer.

This package provides the homeassistant.* namespace that Python integrations expect,
backed by Rust implementations via PyO3.

Strategy:
- Re-export safe modules (constants, types, exceptions) from native HA
- Inherit entity base classes from native HA, override methods that route to Rust
- Implement core types (HomeAssistant, EventBus, etc.) as Rust-backed proxies
- STRICT by default: unknown modules raise ImportError (ensures we know what's ported)
- Set HA_ALLOW_NATIVE_FALLBACK=1 for development to fall back to native HA

The _native_loader is ONLY for loading native modules that we explicitly re-export
(constants, base classes we inherit from). It is NOT a general fallback.
"""

import logging
import os
from pathlib import Path

_LOGGER = logging.getLogger(__name__)

# Check if native fallback is allowed (development mode only)
_ALLOW_NATIVE_FALLBACK = os.environ.get("HA_ALLOW_NATIVE_FALLBACK", "").lower() in ("1", "true", "yes")

if _ALLOW_NATIVE_FALLBACK:
    _LOGGER.warning(
        "HA_ALLOW_NATIVE_FALLBACK is enabled - unknown modules will fall back to native HA. "
        "This should only be used for development."
    )
    # Extend __path__ to include native HA's package directory
    _native_ha = Path(__file__).parents[2] / "vendor/ha-core/homeassistant"
    if _native_ha.exists():
        __path__.append(str(_native_ha))

from homeassistant.core import HomeAssistant, callback

__all__ = ["HomeAssistant", "callback"]
