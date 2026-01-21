"""Home Assistant components shim.

For components that have shim implementations (light, switch, sensor), this package
provides those shims. For other components (actual integrations), the native HA
components directory is included in __path__ so integrations can be loaded.

Note: Component shims (light/, switch/, sensor/) take precedence because they appear
first in __path__. This allows us to intercept entity platform base classes while
still allowing native integration code to run.
"""

import logging
from pathlib import Path

_LOGGER = logging.getLogger(__name__)


# Always extend __path__ to include native HA's components directory.
# This is required for loading Python integrations (like accuweather).
# Component shims in this directory take precedence (they're first in __path__).
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
