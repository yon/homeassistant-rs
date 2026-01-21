"""Merge native Home Assistant template package with Rust implementations.

This shim:
1. Loads the full native template package (__init__.py exports)
2. Imports Rust classes from ha_core_rs.template
3. Extends __path__ to allow submodule imports (render_info, context, etc.)
"""

from pathlib import Path

from homeassistant._native_loader import load_native_module

# Load native template module (the __init__.py)
_native = load_native_module("homeassistant.helpers.template")

# Re-export everything from native
_public_names = []
for _name in dir(_native):
    if _name.startswith("_"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

# Import Rust classes from ha_core_rs (they take precedence)
try:
    from ha_core_rs.template import Template, TemplateEngine

    globals()["Template"] = Template
    globals()["TemplateEngine"] = TemplateEngine
    if "Template" not in _public_names:
        _public_names.append("Template")
    if "TemplateEngine" not in _public_names:
        _public_names.append("TemplateEngine")
except ImportError:
    # ha_core_rs not available (e.g., in pure Python mode)
    pass

__all__ = _public_names

# Extend __path__ to include native HA's template package for submodules
# This allows `from homeassistant.helpers.template.render_info import ...` to work
_native_template_path = getattr(_native, "__path__", None)
if _native_template_path:
    __path__.extend(_native_template_path)
else:
    # Fallback: find native template directory
    def _find_native_template():
        current = Path(__file__).resolve().parent
        for _ in range(10):
            vendor_path = current / "vendor" / "ha-core" / "homeassistant" / "helpers" / "template"
            if vendor_path.exists():
                return vendor_path
            parent = current.parent
            if parent == current:
                break
            current = parent
        # Fallback: try from cwd
        cwd_vendor = Path.cwd() / "vendor" / "ha-core" / "homeassistant" / "helpers" / "template"
        if cwd_vendor.exists():
            return cwd_vendor
        return None

    _native_template = _find_native_template()
    if _native_template:
        __path__.append(str(_native_template))
