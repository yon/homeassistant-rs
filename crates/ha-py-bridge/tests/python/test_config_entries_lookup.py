"""Tests for config entries lookup in config_entries_wrapper.py.

When a config entry is stored via store_config_entry(), async_get_entry()
should return it. This is needed for device_registry.async_get_or_create()
which looks up config entries by ID.
"""

from __future__ import annotations

import sys
import os
import types

# Add embedded_python to path so we can import config_entries_wrapper
sys.path.insert(0, os.path.join(
    os.path.dirname(__file__), '..', '..', 'embedded_python'
))

import config_entries_wrapper


class MockEntry:
    """Minimal mock config entry with entry_id and domain."""

    def __init__(self, entry_id, domain, unique_id=None):
        self.entry_id = entry_id
        self.domain = domain
        self.unique_id = unique_id


def setup_function():
    """Reset global state before each test."""
    config_entries_wrapper._config_entries.clear()


def test_store_and_get_entry():
    """Stored entry should be retrievable by async_get_entry."""
    entry = MockEntry("abc123", "lutron_caseta")
    config_entries_wrapper.store_config_entry(entry)
    result = config_entries_wrapper.async_get_entry("abc123")
    assert result is entry, "async_get_entry should return the stored entry"


def test_get_entry_returns_none_for_unknown():
    """async_get_entry should return None for unknown entry_id."""
    result = config_entries_wrapper.async_get_entry("nonexistent")
    assert result is None, "Should return None for unknown entry"


def test_store_multiple_entries():
    """Multiple entries should all be retrievable."""
    e1 = MockEntry("id1", "hue")
    e2 = MockEntry("id2", "mqtt")
    config_entries_wrapper.store_config_entry(e1)
    config_entries_wrapper.store_config_entry(e2)

    assert config_entries_wrapper.async_get_entry("id1") is e1
    assert config_entries_wrapper.async_get_entry("id2") is e2


def test_async_entries_returns_stored_entries():
    """async_entries() should return all stored entries."""
    e1 = MockEntry("id1", "hue")
    e2 = MockEntry("id2", "mqtt")
    config_entries_wrapper.store_config_entry(e1)
    config_entries_wrapper.store_config_entry(e2)

    entries = config_entries_wrapper.async_entries()
    assert len(entries) == 2


def test_async_entries_filters_by_domain():
    """async_entries(domain=x) should only return entries for that domain."""
    e1 = MockEntry("id1", "hue")
    e2 = MockEntry("id2", "mqtt")
    e3 = MockEntry("id3", "hue")
    config_entries_wrapper.store_config_entry(e1)
    config_entries_wrapper.store_config_entry(e2)
    config_entries_wrapper.store_config_entry(e3)

    hue_entries = config_entries_wrapper.async_entries(domain="hue")
    assert len(hue_entries) == 2
    assert all(e.domain == "hue" for e in hue_entries)


def test_async_entry_for_domain_unique_id():
    """async_entry_for_domain_unique_id should find entry by domain+unique_id."""
    e1 = MockEntry("id1", "hue", unique_id="bridge-001")
    e2 = MockEntry("id2", "mqtt", unique_id="broker-001")
    config_entries_wrapper.store_config_entry(e1)
    config_entries_wrapper.store_config_entry(e2)

    result = config_entries_wrapper.async_entry_for_domain_unique_id("hue", "bridge-001")
    assert result is e1

    result = config_entries_wrapper.async_entry_for_domain_unique_id("hue", "nonexistent")
    assert result is None
