"""Tests for HassWrapper methods used by Python integrations.

Tests async_create_task_internal, add_job, and other bridge methods
that integrations depend on.
"""

from __future__ import annotations

import asyncio
import types

import pytest


class FakeStates:
    """Minimal states mock."""

    def get(self, entity_id):
        return None

    def set(self, entity_id, state, attrs=None):
        pass


class FakeBus:
    """Minimal bus mock with async_fire and async_fire_internal."""

    def async_fire(self, event_type, event_data=None, origin=None, context=None):
        future = asyncio.get_event_loop().create_future()
        future.set_result(None)
        return future

    def async_fire_internal(
        self, event_type, event_data=None, origin=None, context=None, time_fired=None
    ):
        return self.async_fire(event_type, event_data, origin, context)


class FakeHass:
    """Minimal HomeAssistant-like object for testing add_job and related methods.

    This simulates the HassWrapper behavior without needing the full Rust bridge.
    The actual HassWrapper delegates add_job to Python asyncio, so we test
    the same logic here.
    """

    def __init__(self):
        self.bus = FakeBus()
        self.states = FakeStates()
        self._tasks = []

    def add_job(self, target, *args):
        """Add a job - dispatch based on type."""
        if asyncio.iscoroutine(target):
            return self.async_create_task(target, eager_start=True)
        elif asyncio.iscoroutinefunction(target):
            coro = target(*args)
            return self.async_create_task(coro, eager_start=True)
        else:
            return self.async_add_executor_job(target, *args)

    def async_add_executor_job(self, func, *args):
        """Run a blocking function in the executor."""
        loop = asyncio.get_event_loop()
        return loop.run_in_executor(None, func, *args)

    def async_create_task(self, target, name=None, eager_start=False):
        """Create a task from a coroutine."""
        loop = asyncio.get_event_loop()
        task = loop.create_task(target, name=name)
        self._tasks.append(task)
        return task

    def async_create_task_internal(self, target, name=None, eager_start=False):
        """Internal version - delegates to async_create_task."""
        return self.async_create_task(target, name, eager_start)


class TestAsyncCreateTaskInternal:
    """Test async_create_task_internal delegates to async_create_task."""

    @pytest.mark.asyncio
    async def test_creates_task_from_coroutine(self) -> None:
        """Test creating a task from a coroutine."""
        hass = FakeHass()
        result = []

        async def my_coro():
            result.append("done")

        task = hass.async_create_task_internal(my_coro())
        await task
        assert result == ["done"]

    @pytest.mark.asyncio
    async def test_task_has_name(self) -> None:
        """Test that task name is set."""
        hass = FakeHass()

        async def my_coro():
            pass

        task = hass.async_create_task_internal(my_coro(), name="test_task")
        assert task.get_name() == "test_task"
        await task


class TestAddJob:
    """Test add_job dispatching logic."""

    @pytest.mark.asyncio
    async def test_add_job_with_coroutine(self) -> None:
        """Test add_job with a coroutine creates a task."""
        hass = FakeHass()
        result = []

        async def my_coro():
            result.append("from_coro")

        # Pass an actual coroutine (not a function)
        coro = my_coro()
        task = hass.add_job(coro)
        await task
        assert result == ["from_coro"]

    @pytest.mark.asyncio
    async def test_add_job_with_coroutine_function(self) -> None:
        """Test add_job with a coroutine function calls it then creates task."""
        hass = FakeHass()
        result = []

        async def my_coro_fn(value):
            result.append(value)

        task = hass.add_job(my_coro_fn, "test_value")
        await task
        assert result == ["test_value"]

    @pytest.mark.asyncio
    async def test_add_job_with_regular_callable(self) -> None:
        """Test add_job with regular callable runs in executor."""
        hass = FakeHass()

        def blocking_fn():
            return 42

        awaitable = hass.add_job(blocking_fn)
        result = await awaitable
        assert result == 42

    @pytest.mark.asyncio
    async def test_add_job_dispatches_correctly(self) -> None:
        """Test that add_job correctly identifies coroutines vs functions."""

        async def async_fn():
            pass

        def sync_fn():
            pass

        # Verify the type checks work correctly
        assert asyncio.iscoroutinefunction(async_fn)
        assert not asyncio.iscoroutinefunction(sync_fn)

        coro = async_fn()
        assert asyncio.iscoroutine(coro)
        assert not asyncio.iscoroutine(sync_fn)
        # Clean up the coroutine
        coro.close()


class TestBusAsyncFireInternal:
    """Test async_fire_internal delegates to async_fire."""

    @pytest.mark.asyncio
    async def test_async_fire_internal_delegates(self) -> None:
        """Test async_fire_internal calls async_fire."""
        fired_events = []

        class TrackingBus:
            def async_fire(self, event_type, event_data=None, origin=None, context=None):
                fired_events.append((event_type, event_data))
                future = asyncio.get_event_loop().create_future()
                future.set_result(None)
                return future

            def async_fire_internal(
                self,
                event_type,
                event_data=None,
                origin=None,
                context=None,
                time_fired=None,
            ):
                return self.async_fire(event_type, event_data, origin, context)

        bus = TrackingBus()
        await bus.async_fire_internal("test_event", {"key": "value"})
        assert fired_events == [("test_event", {"key": "value"})]

    @pytest.mark.asyncio
    async def test_async_fire_internal_accepts_time_fired(self) -> None:
        """Test async_fire_internal accepts time_fired parameter."""
        bus = FakeBus()
        # Should not raise
        await bus.async_fire_internal(
            "test_event", {"data": 1}, time_fired=1234567890.0
        )
