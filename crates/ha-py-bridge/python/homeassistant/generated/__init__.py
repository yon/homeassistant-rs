"""Re-export generated modules from native Home Assistant.

The generated module contains auto-generated data (config flows, countries, etc.)
which is safe to use directly from native HA.
"""

from pathlib import Path

# Always extend __path__ to include native HA's generated directory
def _find_native_generated():
    current = Path(__file__).resolve().parent
    for _ in range(10):
        vendor_path = current / "vendor" / "ha-core" / "homeassistant" / "generated"
        if vendor_path.exists():
            return vendor_path
        parent = current.parent
        if parent == current:
            break
        current = parent
    # Fallback: try from cwd
    cwd_vendor = Path.cwd() / "vendor" / "ha-core" / "homeassistant" / "generated"
    if cwd_vendor.exists():
        return cwd_vendor
    return None


_native_generated = _find_native_generated()
if _native_generated:
    __path__.append(str(_native_generated))
