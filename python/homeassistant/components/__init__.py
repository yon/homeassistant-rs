"""Home Assistant components shim.

For components that have shim implementations (light, switch, sensor), this package
provides those shims. Unknown components raise ImportError unless HA_ALLOW_NATIVE_FALLBACK=1.
"""

import logging
import os
from pathlib import Path

_LOGGER = logging.getLogger(__name__)

# Check if native fallback is allowed (development mode only)
_ALLOW_NATIVE_FALLBACK = os.environ.get("HA_ALLOW_NATIVE_FALLBACK", "").lower() in ("1", "true", "yes")

if _ALLOW_NATIVE_FALLBACK:
    # Extend __path__ to include native HA's components directory
    _native_components = Path(__file__).parents[3] / "vendor/ha-core/homeassistant/components"
    if _native_components.exists():
        __path__.append(str(_native_components))
