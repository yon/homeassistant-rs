//! Event bus with typed pub/sub for Home Assistant
//!
//! This crate provides the EventBus, which is the central message broker
//! for Home Assistant. Components can subscribe to events and fire events
//! to communicate asynchronously.

use dashmap::DashMap;
use ha_core::{Context, Event, EventData, EventType};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, trace};

/// Default channel capacity for event subscriptions
const DEFAULT_CHANNEL_CAPACITY: usize = 1024;

/// A unique identifier for an event listener
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListenerId(u64);

/// The event bus for publishing and subscribing to events
///
/// The EventBus is the central message broker in Home Assistant. It supports:
/// - Subscribing to specific event types
/// - Subscribing to all events (MATCH_ALL)
/// - Firing events to all subscribers
/// - Typed event subscriptions for type-safe event handling
pub struct EventBus {
    /// Map of event types to their broadcast senders
    listeners: DashMap<EventType, broadcast::Sender<Event<serde_json::Value>>>,
    /// Special sender for MATCH_ALL subscribers
    match_all_sender: broadcast::Sender<Event<serde_json::Value>>,
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
            next_listener_id: AtomicU64::new(1),
            capacity,
        }
    }

    /// Subscribe to events of a specific type
    ///
    /// Returns a receiver that will receive all events of the given type.
    pub fn subscribe(
        &self,
        event_type: impl Into<EventType>,
    ) -> broadcast::Receiver<Event<serde_json::Value>> {
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
    pub fn subscribe_all(&self) -> broadcast::Receiver<Event<serde_json::Value>> {
        self.match_all_sender.subscribe()
    }

    /// Fire an event to all subscribers
    ///
    /// The event will be delivered to:
    /// 1. All subscribers of the specific event type
    /// 2. All MATCH_ALL subscribers
    pub fn fire(&self, event: Event<serde_json::Value>) {
        debug!(event_type = %event.event_type, "Firing event");

        // Send to specific event type subscribers
        if let Some(sender) = self.listeners.get(&event.event_type) {
            // Ignore send errors - they just mean no active receivers
            let _ = sender.send(event.clone());
        }

        // Send to MATCH_ALL subscribers
        let _ = self.match_all_sender.send(event);
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

    /// Generate a new unique listener ID
    pub fn next_listener_id(&self) -> ListenerId {
        ListenerId(self.next_listener_id.fetch_add(1, Ordering::SeqCst))
    }

    /// Get the number of active event type subscriptions
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// A receiver for typed events
pub struct TypedEventReceiver<T> {
    rx: broadcast::Receiver<Event<serde_json::Value>>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: EventData + serde::de::DeserializeOwned> TypedEventReceiver<T> {
    fn new(rx: broadcast::Receiver<Event<serde_json::Value>>) -> Self {
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
            let event = self.rx.recv().await?;
            if let Ok(data) = serde_json::from_value::<T>(event.data.clone()) {
                return Ok(Event {
                    event_type: event.event_type,
                    data,
                    origin: event.origin,
                    time_fired: event.time_fired,
                    context: event.context,
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
    use ha_core::events::StateChangedData;
    use ha_core::{EntityId, State};
    use serde_json::json;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_subscribe_and_fire() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe("test_event");

        let ctx = Context::new();
        let event = Event::new("test_event", json!({"key": "value"}), ctx);
        bus.fire(event.clone());

        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_type.as_str(), "test_event");
        assert_eq!(received.data["key"], "value");
    }

    #[tokio::test]
    async fn test_match_all_subscription() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe_all();

        let ctx = Context::new();
        bus.fire(Event::new("event_a", json!({}), ctx.clone()));
        bus.fire(Event::new("event_b", json!({}), ctx));

        let event1 = rx.recv().await.unwrap();
        let event2 = rx.recv().await.unwrap();

        assert_eq!(event1.event_type.as_str(), "event_a");
        assert_eq!(event2.event_type.as_str(), "event_b");
    }

    #[tokio::test]
    async fn test_typed_subscription() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe_typed::<StateChangedData>();

        let entity_id = EntityId::new("light", "test").unwrap();
        let new_state = State::new(entity_id.clone(), "on", HashMap::new(), Context::new());

        let data = StateChangedData {
            entity_id,
            old_state: None,
            new_state: Some(new_state),
        };

        bus.fire_typed(data.clone(), Context::new());

        let received = rx.recv().await.unwrap();
        assert_eq!(received.data.entity_id.to_string(), "light.test");
        assert!(received.data.new_state.is_some());
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new();
        let mut rx1 = bus.subscribe("test_event");
        let mut rx2 = bus.subscribe("test_event");

        let ctx = Context::new();
        bus.fire(Event::new("test_event", json!({"n": 1}), ctx));

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();

        assert_eq!(e1.data["n"], 1);
        assert_eq!(e2.data["n"], 1);
    }

    #[tokio::test]
    async fn test_no_cross_event_pollution() {
        let bus = EventBus::new();
        let mut rx_a = bus.subscribe("event_a");
        let mut rx_b = bus.subscribe("event_b");

        let ctx = Context::new();
        bus.fire(Event::new("event_a", json!({"type": "a"}), ctx));

        // rx_a should receive the event
        let received = rx_a.recv().await.unwrap();
        assert_eq!(received.data["type"], "a");

        // rx_b should not receive anything (would timeout in a real test)
        // We can verify by checking if try_recv returns empty
        assert!(rx_b.try_recv().is_err());
    }

    #[test]
    fn test_listener_id_uniqueness() {
        let bus = EventBus::new();
        let id1 = bus.next_listener_id();
        let id2 = bus.next_listener_id();
        let id3 = bus.next_listener_id();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }
}
