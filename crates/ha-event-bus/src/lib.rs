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

/// The event bus for publishing and subscribing to events
///
/// The EventBus is the central message broker in Home Assistant. It supports:
/// - Subscribing to specific event types
/// - Subscribing to all events (MATCH_ALL)
/// - Firing events to all subscribers
/// - Typed event subscriptions for type-safe event handling
///
/// Events are wrapped in `Arc` to avoid cloning for each subscriber.
pub struct EventBus {
    /// Map of event types to their broadcast senders (Arc-wrapped events)
    listeners: DashMap<EventType, broadcast::Sender<ArcEvent>>,
    /// Special sender for MATCH_ALL subscribers (Arc-wrapped events)
    match_all_sender: broadcast::Sender<ArcEvent>,
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
    /// 1. All subscribers of the specific event type
    /// 2. All MATCH_ALL subscribers (unless event type is excluded)
    ///
    /// The event is wrapped in Arc internally, so subscribers receive
    /// cheap Arc clones instead of full event clones.
    ///
    /// Certain high-frequency or internal events are excluded from MATCH_ALL
    /// to prevent unnecessary load on general subscribers.
    pub fn fire(&self, event: Event<serde_json::Value>) {
        debug!(event_type = %event.event_type, "Firing event");

        // Wrap event in Arc once, then share among all subscribers
        let arc_event = Arc::new(event);

        // Send to specific event type subscribers
        if let Some(sender) = self.listeners.get(&arc_event.event_type) {
            // Ignore send errors - they just mean no active receivers
            // Arc::clone is cheap (atomic increment)
            let _ = sender.send(Arc::clone(&arc_event));
        }

        // Send to MATCH_ALL subscribers, unless this event type is excluded
        // Matches Python HA's EVENTS_EXCLUDED_FROM_MATCH_ALL
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

// Unit tests removed - covered by HA native tests via `make ha-compat-test`
// See tests/ha_compat/ for comprehensive EventBus testing through Python bindings
