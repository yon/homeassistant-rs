"""Home Assistant helpers shim.

For helpers that have shim implementations (entity, entity_platform, typing), this package
provides those shims. For other helpers, the native HA helpers directory is included
in __path__ so they can be imported.

Note: Helper shims (entity.py, entity_platform.py, typing.py) take precedence because
they're modules directly in this directory. This allows us to intercept key helpers
while still allowing other native helpers to be used.
"""

import logging
from pathlib import Path

_LOGGER = logging.getLogger(__name__)


# Always extend __path__ to include native HA's helpers directory.
# This allows integrations to import helpers we haven't shimmed.
# Helper shims in this directory take precedence (they're files here, not in native).
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
