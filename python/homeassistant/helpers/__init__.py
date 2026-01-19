"""Home Assistant helpers shim.

For helpers that have shim implementations (entity, entity_platform, typing), this package
provides those shims. Unknown helpers raise ImportError unless HA_ALLOW_NATIVE_FALLBACK=1.
"""

import logging
import os
from pathlib import Path

_LOGGER = logging.getLogger(__name__)

# Check if native fallback is allowed (development mode only)
_ALLOW_NATIVE_FALLBACK = os.environ.get("HA_ALLOW_NATIVE_FALLBACK", "").lower() in ("1", "true", "yes")

if _ALLOW_NATIVE_FALLBACK:
    # Extend __path__ to include native HA's helpers directory
    _native_helpers = Path(__file__).parents[3] / "vendor/ha-core/homeassistant/helpers"
    if _native_helpers.exists():
        __path__.append(str(_native_helpers))
