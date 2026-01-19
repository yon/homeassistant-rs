"""Home Assistant components shim.

For components that have shim implementations (light, switch, sensor), this package
provides those shims. Unknown components raise ImportError unless ALLOW_HA_NATIVE_FALLBACK=1.
"""

import logging
import os
from pathlib import Path

_LOGGER = logging.getLogger(__name__)

# Check if native fallback is allowed (development mode only)
_ALLOW_HA_NATIVE_FALLBACK = os.environ.get("ALLOW_HA_NATIVE_FALLBACK", "").lower() in ("1", "true", "yes")

if _ALLOW_HA_NATIVE_FALLBACK:
    # Extend __path__ to include native HA's components directory
    # Search upward to find vendor/ha-core
    def _find_native_components():
        current = Path(__file__).resolve().parent
        for _ in range(10):
            vendor_path = current / "vendor" / "ha-core" / "homeassistant" / "components"
            if vendor_path.exists():
                return vendor_path
            parent = current.parent
            if parent == current:
                break
            current = parent
        # Fallback: try from cwd
        cwd_vendor = Path.cwd() / "vendor" / "ha-core" / "homeassistant" / "components"
        if cwd_vendor.exists():
            return cwd_vendor
        return None

    _native_components = _find_native_components()
    if _native_components:
        __path__.append(str(_native_components))
