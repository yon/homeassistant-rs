"""Light component shim - inherits from Python HA, uses RustStateMixin for state writes.

LightEntity inherits from native HA's LightEntity (gets all _attr_* properties,
ColorMode support, brightness handling) and RustStateMixin (gets Rust state routing).
"""

from __future__ import annotations

from homeassistant._native_loader import load_native_module
from homeassistant.helpers.entity import RustStateMixin

# Load native HA light module
_native = load_native_module("homeassistant.components.light")

# Get the metaclass from the native class to avoid metaclass conflicts
# when the native module is loaded fresh with a new CachedProperties instance
_LightEntityMeta = type(_native.LightEntity)


class LightEntity(RustStateMixin, _native.LightEntity, metaclass=_LightEntityMeta):
    """Light entity that routes state writes to Rust.

    Inherits from:
    - RustStateMixin: provides async_write_ha_state() routing to Rust
    - native LightEntity: gets all _attr_* properties, ColorMode support

    RustStateMixin must come first in the MRO so its async_write_ha_state
    takes precedence over the native implementation.
    """

    pass


# Re-export everything from native module (ColorMode, ATTR_*, SUPPORT_*, etc.)
# These are enums and constants, safe to re-export
ColorMode = _native.ColorMode
LightEntityFeature = _native.LightEntityFeature

# Re-export all constants
ATTR_BRIGHTNESS = _native.ATTR_BRIGHTNESS
ATTR_COLOR_MODE = _native.ATTR_COLOR_MODE
ATTR_COLOR_TEMP_KELVIN = _native.ATTR_COLOR_TEMP_KELVIN
ATTR_EFFECT = _native.ATTR_EFFECT
ATTR_EFFECT_LIST = _native.ATTR_EFFECT_LIST
ATTR_HS_COLOR = _native.ATTR_HS_COLOR
ATTR_MAX_COLOR_TEMP_KELVIN = _native.ATTR_MAX_COLOR_TEMP_KELVIN
ATTR_MIN_COLOR_TEMP_KELVIN = _native.ATTR_MIN_COLOR_TEMP_KELVIN
ATTR_RGB_COLOR = _native.ATTR_RGB_COLOR
ATTR_RGBW_COLOR = _native.ATTR_RGBW_COLOR
ATTR_RGBWW_COLOR = _native.ATTR_RGBWW_COLOR
ATTR_SUPPORTED_COLOR_MODES = _native.ATTR_SUPPORTED_COLOR_MODES
ATTR_XY_COLOR = _native.ATTR_XY_COLOR

# Domain
DOMAIN = _native.DOMAIN

# Build __all__ from all public names we've defined
__all__ = [
    "LightEntity",
    "ColorMode",
    "LightEntityFeature",
    "ATTR_BRIGHTNESS",
    "ATTR_COLOR_MODE",
    "ATTR_COLOR_TEMP_KELVIN",
    "ATTR_EFFECT",
    "ATTR_EFFECT_LIST",
    "ATTR_HS_COLOR",
    "ATTR_MAX_COLOR_TEMP_KELVIN",
    "ATTR_MIN_COLOR_TEMP_KELVIN",
    "ATTR_RGB_COLOR",
    "ATTR_RGBW_COLOR",
    "ATTR_RGBWW_COLOR",
    "ATTR_SUPPORTED_COLOR_MODES",
    "ATTR_XY_COLOR",
    "DOMAIN",
]

# Also re-export any other public names we might have missed
for _name in dir(_native):
    if not _name.startswith("_") and _name not in globals():
        globals()[_name] = getattr(_native, _name)
        __all__.append(_name)
