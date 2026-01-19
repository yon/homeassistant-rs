"""Home Assistant helpers shim.

For helpers that have shim implementations (entity, entity_platform, typing), this package
provides those shims. Unknown helpers raise ImportError unless ALLOW_HA_NATIVE_FALLBACK=1.
"""

import logging
import os
from pathlib import Path

_LOGGER = logging.getLogger(__name__)

# Check if native fallback is allowed (development mode only)
_ALLOW_HA_NATIVE_FALLBACK = os.environ.get("ALLOW_HA_NATIVE_FALLBACK", "").lower() in ("1", "true", "yes")

if _ALLOW_HA_NATIVE_FALLBACK:
    # Extend __path__ to include native HA's helpers directory
    # Search upward to find vendor/ha-core
    def _find_native_helpers():
        current = Path(__file__).resolve().parent
        for _ in range(10):
            vendor_path = current / "vendor" / "ha-core" / "homeassistant" / "helpers"
            if vendor_path.exists():
                return vendor_path
            parent = current.parent
            if parent == current:
                break
            current = parent
        # Fallback: try from cwd
        cwd_vendor = Path.cwd() / "vendor" / "ha-core" / "homeassistant" / "helpers"
        if cwd_vendor.exists():
            return cwd_vendor
        return None

    _native_helpers = _find_native_helpers()
    if _native_helpers:
        __path__.append(str(_native_helpers))
