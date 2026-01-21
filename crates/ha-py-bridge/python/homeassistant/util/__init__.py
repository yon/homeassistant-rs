"""Re-export all utilities from native Home Assistant.

The util module contains pure utility functions with no HA-specific state,
safe to re-export directly. We also extend __path__ to include the native
util directory so submodules (util.dt, util.color, etc.) are found.
"""

from homeassistant._native_loader import load_native_module

# Load native HA util module
_native = load_native_module("homeassistant.util")

# Extend __path__ to include native util directory for submodule access
# This allows imports like "from homeassistant.util.dt import utcnow"
import os.path

_native_util_path = os.path.dirname(_native.__file__)
if _native_util_path not in __path__:
    __path__.append(_native_util_path)

# Re-export everything from native util
_public_names = []
for _name in dir(_native):
    if _name.startswith("_"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

__all__ = _public_names
