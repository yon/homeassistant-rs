//! Event bus with typed pub/sub for Home Assistant
//!
//! This crate provides the EventBus, which is the central message broker
//! for Home Assistant. Components can subscribe to events and fire events
//! to communicate asynchronously.
//!
//! Events are wrapped in `Arc` to avoid cloning event data for each subscriber.
//! This is a significant optimization for events with large JSON payloads.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::broadcast;
use tracing::{debug, trace};

use ha_core::events::{HOMEASSISTANT_CLOSE, STATE_REPORTED};
use ha_core::{Context, Event, EventData, EventType};

/// Default channel capacity for event subscriptions
const DEFAULT_CHANNEL_CAPACITY: usize = 1024;

/// A unique identifier for an event listener
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListenerId(u64);

/// Arc-wrapped event type for efficient broadcasting
///
/// Events are wrapped in Arc to avoid cloning the entire event (including JSON data)
/// for each subscriber. Instead, subscribers receive a cheap Arc clone.
pub type ArcEvent = Arc<Event<serde_json::Value>>;

/// Type for synchronous event callbacks that fire inline during `fire()`.
/// These are called on the same thread, before broadcast channel delivery.
pub type SyncCallback = Arc<dyn Fn(&Event<serde_json::Value>) + Send + Sync>;

/// The event bus for publishing and subscribing to events
///
/// The EventBus is the central message broker in Home Assistant. It supports:
/// - Subscribing to specific event types (async broadcast channels)
/// - Subscribing to all events (MATCH_ALL)
/// - Synchronous callbacks that fire inline during `fire()`
/// - Typed event subscriptions for type-safe event handling
///
/// Events are wrapped in `Arc` to avoid cloning for each subscriber.
pub struct EventBus {
    /// Map of event types to their broadcast senders (Arc-wrapped events)
    listeners: DashMap<EventType, broadcast::Sender<ArcEvent>>,
    /// Special sender for MATCH_ALL subscribers (Arc-wrapped events)
    match_all_sender: broadcast::Sender<ArcEvent>,
    /// Synchronous callbacks per event type (called inline during fire).
    /// Use EventType::match_all() as key for MATCH_ALL listeners.
    sync_listeners: DashMap<EventType, Vec<(ListenerId, SyncCallback)>>,
    /// Counter for generating unique listener IDs
    next_listener_id: AtomicU64,
    /// Channel capacity
    capacity: usize,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CHANNEL_CAPACITY)
    }

    /// Create a new event bus with specified channel capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let (match_all_sender, _) = broadcast::channel(capacity);
        Self {
            listeners: DashMap::new(),
            match_all_sender,
            sync_listeners: DashMap::new(),
            next_listener_id: AtomicU64::new(1),
            capacity,
        }
    }

    /// Subscribe to events of a specific type
    ///
    /// Returns a receiver that will receive Arc-wrapped events of the given type.
    /// Using Arc avoids cloning the event data for each subscriber.
    pub fn subscribe(&self, event_type: impl Into<EventType>) -> broadcast::Receiver<ArcEvent> {
        let event_type = event_type.into();
        trace!(event_type = %event_type, "Subscribing to event type");

        if event_type.is_match_all() {
            return self.match_all_sender.subscribe();
        }

        self.listeners
            .entry(event_type.clone())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(self.capacity);
                tx
            })
            .subscribe()
    }

    /// Subscribe to events of a specific typed event
    ///
    /// Returns a receiver that will receive events with parsed data.
    pub fn subscribe_typed<T: EventData + serde::de::DeserializeOwned>(
        &self,
    ) -> TypedEventReceiver<T> {
        let rx = self.subscribe(T::event_type());
        TypedEventReceiver::new(rx)
    }

    /// Subscribe to all events
    ///
    /// Returns a receiver that will receive Arc-wrapped events.
    /// Using Arc avoids cloning the event data for each subscriber.
    pub fn subscribe_all(&self) -> broadcast::Receiver<ArcEvent> {
        self.match_all_sender.subscribe()
    }

    /// Fire an event to all subscribers
    ///
    /// The event will be delivered to:
    /// 1. Synchronous callbacks for the specific event type (inline)
    /// 2. Synchronous MATCH_ALL callbacks (inline, unless excluded)
    /// 3. Async broadcast subscribers for the specific event type
    /// 4. Async MATCH_ALL broadcast subscribers (unless excluded)
    ///
    /// Synchronous callbacks fire first, inline on the calling thread.
    /// The event is then wrapped in Arc for broadcast channel delivery.
    pub fn fire(&self, event: Event<serde_json::Value>) {
        debug!(event_type = %event.event_type, "Firing event");

        // Fire synchronous callbacks for this event type
        if let Some(listeners) = self.sync_listeners.get(&event.event_type) {
            for (_, callback) in listeners.iter() {
                callback(&event);
            }
        }

        // Fire synchronous MATCH_ALL callbacks (unless excluded)
        if !Self::is_excluded_from_match_all(&event.event_type) {
            if let Some(listeners) = self.sync_listeners.get(&EventType::match_all()) {
                for (_, callback) in listeners.iter() {
                    callback(&event);
                }
            }
        }

        // Wrap event in Arc for broadcast channel delivery
        let arc_event = Arc::new(event);

        // Send to specific event type subscribers
        if let Some(sender) = self.listeners.get(&arc_event.event_type) {
            let _ = sender.send(Arc::clone(&arc_event));
        }

        // Send to MATCH_ALL subscribers, unless this event type is excluded
        if !Self::is_excluded_from_match_all(&arc_event.event_type) {
            let _ = self.match_all_sender.send(arc_event);
        }
    }

    /// Check if an event type is excluded from MATCH_ALL delivery
    ///
    /// Matches Python HA's EVENTS_EXCLUDED_FROM_MATCH_ALL:
    /// - EVENT_HOMEASSISTANT_CLOSE
    /// - EVENT_STATE_REPORTED
    fn is_excluded_from_match_all(event_type: &EventType) -> bool {
        matches!(event_type.as_str(), HOMEASSISTANT_CLOSE | STATE_REPORTED)
    }

    /// Fire a typed event
    pub fn fire_typed<T: EventData + serde::Serialize>(&self, data: T, context: Context) {
        let event = Event::typed(data, context);
        let json_data = serde_json::to_value(&event.data).unwrap_or_default();
        let event = Event {
            event_type: event.event_type,
            data: json_data,
            origin: event.origin,
            time_fired: event.time_fired,
            context: event.context,
        };
        self.fire(event);
    }

    /// Register a synchronous callback for an event type.
    ///
    /// The callback fires inline during `fire()` on the calling thread.
    /// Use `EventType::match_all()` to listen to all events.
    /// Returns a ListenerId that can be used to remove the listener.
    pub fn listen_sync(
        &self,
        event_type: impl Into<EventType>,
        callback: SyncCallback,
    ) -> ListenerId {
        let event_type = event_type.into();
        let id = self.next_listener_id();
        self.sync_listeners
            .entry(event_type)
            .or_default()
            .push((id, callback));
        id
    }

    /// Remove a synchronous callback by its ListenerId.
    pub fn remove_sync_listener(&self, listener_id: ListenerId) {
        // Search all event types for this listener
        for mut entry in self.sync_listeners.iter_mut() {
            let listeners = entry.value_mut();
            if let Some(pos) = listeners.iter().position(|(id, _)| *id == listener_id) {
                listeners.swap_remove(pos);
                return;
            }
        }
    }

    /// Generate a new unique listener ID
    pub fn next_listener_id(&self) -> ListenerId {
        ListenerId(self.next_listener_id.fetch_add(1, Ordering::SeqCst))
    }

    /// Get the number of active event type subscriptions
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }

    /// Get the number of sync listeners across all event types
    pub fn sync_listener_count(&self) -> usize {
        self.sync_listeners
            .iter()
            .map(|entry| entry.value().len())
            .sum()
    }

    /// Iterate over sync listeners, returning (EventType, count) pairs
    pub fn sync_listeners_iter(&self) -> Vec<(EventType, usize)> {
        self.sync_listeners
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().len()))
            .collect()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// A receiver for typed events
pub struct TypedEventReceiver<T> {
    rx: broadcast::Receiver<ArcEvent>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: EventData + serde::de::DeserializeOwned> TypedEventReceiver<T> {
    fn new(rx: broadcast::Receiver<ArcEvent>) -> Self {
        Self {
            rx,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Receive the next typed event
    ///
    /// Returns None if the event data couldn't be deserialized.
    pub async fn recv(&mut self) -> Result<Event<T>, broadcast::error::RecvError> {
        loop {
            let arc_event = self.rx.recv().await?;
            if let Ok(data) = serde_json::from_value::<T>(arc_event.data.clone()) {
                return Ok(Event {
                    event_type: arc_event.event_type.clone(),
                    data,
                    origin: arc_event.origin, // Copy, not Clone
                    time_fired: arc_event.time_fired,
                    context: arc_event.context.clone(),
                });
            }
            // If deserialization failed, try the next event
        }
    }
}

/// Thread-safe wrapper for EventBus
pub type SharedEventBus = Arc<EventBus>;

#[cfg(test)]
mod tests {
    use super::*;
    use ha_core::events::HOMEASSISTANT_CLOSE;
    use ha_core::{Context, Event};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn make_event(event_type: &str) -> Event<serde_json::Value> {
        Event::new(event_type, serde_json::Value::Null, Context::default())
    }

    #[tokio::test]
    async fn subscribe_receives_fired_event() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe("test_event");
        bus.fire(make_event("test_event"));
        let event = rx.recv().await.expect("should receive event");
        assert_eq!(event.event_type.as_str(), "test_event");
    }

    #[tokio::test]
    async fn subscribe_does_not_receive_other_event_types() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe("type_a");
        bus.fire(make_event("type_b"));
        // Channel should have no messages for type_a
        let result = rx.try_recv();
        assert!(
            result.is_err(),
            "should not receive event of different type"
        );
    }

    #[tokio::test]
    async fn subscribe_all_receives_events() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe_all();
        bus.fire(make_event("any_event"));
        let event = rx.recv().await.expect("should receive event");
        assert_eq!(event.event_type.as_str(), "any_event");
    }

    #[tokio::test]
    async fn subscribe_all_excludes_homeassistant_close() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe_all();
        bus.fire(make_event(HOMEASSISTANT_CLOSE));
        let result = rx.try_recv();
        assert!(
            result.is_err(),
            "HOMEASSISTANT_CLOSE should be excluded from match_all"
        );
    }

    #[test]
    fn sync_listener_fires_inline() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);
        bus.listen_sync(
            "sync_test",
            Arc::new(move |_| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            }),
        );
        bus.fire(make_event("sync_test"));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn remove_sync_listener_stops_delivery() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);
        let id = bus.listen_sync(
            "sync_test",
            Arc::new(move |_| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            }),
        );
        bus.fire(make_event("sync_test"));
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        bus.remove_sync_listener(id);
        bus.fire(make_event("sync_test"));
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "should not fire after removal"
        );
    }

    #[test]
    fn listener_count_tracks_subscriptions() {
        let bus = EventBus::new();
        assert_eq!(bus.listener_count(), 0);
        let _rx = bus.subscribe("event_a");
        assert_eq!(bus.listener_count(), 1);
        let _rx2 = bus.subscribe("event_b");
        assert_eq!(bus.listener_count(), 2);
    }

    #[test]
    fn sync_listener_count_tracks_sync_listeners() {
        let bus = EventBus::new();
        assert_eq!(bus.sync_listener_count(), 0);
        bus.listen_sync("event_a", Arc::new(|_| {}));
        assert_eq!(bus.sync_listener_count(), 1);
        bus.listen_sync("event_b", Arc::new(|_| {}));
        assert_eq!(bus.sync_listener_count(), 2);
    }

    #[test]
    fn default_creates_event_bus() {
        let bus = EventBus::default();
        assert_eq!(bus.listener_count(), 0);
    }
}
