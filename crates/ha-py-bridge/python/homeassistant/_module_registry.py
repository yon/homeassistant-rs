"""Module registry for tracking shim vs vendor modules.

This module provides a type-safe way to distinguish between shim modules
(our implementations) and vendor modules (native Home Assistant).

Usage:
    from homeassistant._module_registry import registry, ModuleSource

    # Check where a module should come from
    if registry.source("homeassistant.const") == ModuleSource.SHIM:
        # Load from shim
    else:
        # Load from vendor

    # Get all shim module names
    for name in registry.shim_modules:
        ...
"""

from __future__ import annotations

import sys
from dataclasses import dataclass
from enum import Enum, auto
from pathlib import Path
from types import ModuleType
from typing import Iterator


class ModuleSource(Enum):
    """Where a module should be loaded from."""

    SHIM = auto()  # Our implementation in crates/ha-py-bridge/python/
    VENDOR = auto()  # Native HA in vendor/ha-core/


@dataclass(frozen=True)
class ModuleInfo:
    """Information about a module's source and location."""

    name: str
    source: ModuleSource

    @property
    def is_shim(self) -> bool:
        return self.source == ModuleSource.SHIM

    @property
    def is_vendor(self) -> bool:
        return self.source == ModuleSource.VENDOR


class ModuleRegistry:
    """Registry tracking which modules are shims vs vendor.

    This is the single source of truth for module sources. It auto-discovers
    shim modules by scanning the shim directory at initialization.
    """

    def __init__(self, shim_path: Path):
        self._shim_path = shim_path
        self._shim_modules: frozenset[str] = self._discover_shim_modules()

    def _discover_shim_modules(self) -> frozenset[str]:
        """Scan the shim directory to find all shimmed modules."""
        modules = set()
        ha_path = self._shim_path / "homeassistant"

        if not ha_path.exists():
            return frozenset()

        for py_file in ha_path.rglob("*.py"):
            if "__pycache__" in str(py_file):
                continue

            rel_path = py_file.relative_to(self._shim_path)
            parts = list(rel_path.parts)

            if parts[-1] == "__init__.py":
                parts = parts[:-1]
            else:
                parts[-1] = parts[-1][:-3]

            # Skip private modules
            if parts and parts[-1].startswith("_"):
                continue

            module_name = ".".join(parts)
            if module_name:
                modules.add(module_name)

        return frozenset(modules)

    @property
    def shim_modules(self) -> frozenset[str]:
        """All module names that have shim implementations."""
        return self._shim_modules

    @property
    def shim_path(self) -> Path:
        """Path to the shim directory."""
        return self._shim_path

    def info(self, module_name: str) -> ModuleInfo:
        """Get full info for a module."""
        return ModuleInfo(name=module_name, source=self.source(module_name))

    def is_shim(self, module_name: str) -> bool:
        """Check if a module should come from shim."""
        return module_name in self._shim_modules

    def is_vendor(self, module_name: str) -> bool:
        """Check if a module should come from vendor."""
        return module_name not in self._shim_modules

    def iter_shim_modules(self) -> Iterator[ModuleInfo]:
        """Iterate over all shim module infos."""
        for name in sorted(self._shim_modules):
            yield ModuleInfo(name=name, source=ModuleSource.SHIM)

    def source(self, module_name: str) -> ModuleSource:
        """Get the source for a module name."""
        if module_name in self._shim_modules:
            return ModuleSource.SHIM
        return ModuleSource.VENDOR

    def verify_loaded_module(self, module_name: str) -> ModuleInfo | None:
        """Verify a loaded module is from the correct source.

        Returns ModuleInfo if correctly loaded, None if not loaded,
        raises ValueError if loaded from wrong source.
        """
        if module_name not in sys.modules:
            return None

        mod = sys.modules[module_name]
        origin = getattr(mod, "__file__", None)
        expected_source = self.source(module_name)

        if origin is None:
            return ModuleInfo(name=module_name, source=expected_source)

        is_from_shim = str(self._shim_path) in origin
        is_from_vendor = "vendor/ha-core" in origin

        if expected_source == ModuleSource.SHIM and is_from_vendor:
            raise ValueError(
                f"Module {module_name} should be from SHIM but loaded from vendor: {origin}"
            )
        if expected_source == ModuleSource.VENDOR and is_from_shim:
            raise ValueError(
                f"Module {module_name} should be from VENDOR but loaded from shim: {origin}"
            )

        actual_source = ModuleSource.SHIM if is_from_shim else ModuleSource.VENDOR
        return ModuleInfo(name=module_name, source=actual_source)


# Global registry instance
_SHIM_PATH = Path(__file__).resolve().parent.parent
registry = ModuleRegistry(_SHIM_PATH)

# Convenience exports
ModuleSource = ModuleSource
