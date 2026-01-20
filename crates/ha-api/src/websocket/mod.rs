//! Home Assistant WebSocket API
//!
//! Implements the Home Assistant WebSocket API for real-time communication.
//! Protocol: https://developers.home-assistant.io/docs/api/websocket
//!
//! This module is organized into:
//! - `types` - Message type definitions (IncomingMessage, OutgoingMessage, etc.)
//! - `connection` - Connection handling and authentication
//! - `dispatch` - Message routing to handlers
//! - `handlers` - Individual command handlers

mod connection;
mod dispatch;
mod handlers;
mod types;

use axum::{
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
};

use crate::AppState;

// Re-export public types for external use and tests
#[allow(unused_imports)]
pub use connection::ActiveConnection;
#[allow(unused_imports)]
pub use types::{
    AuthInvalidMessage, AuthOkMessage, AuthRequiredMessage, EntityIds, ErrorInfo, EventMessage,
    IncomingMessage, OutgoingMessage, PongMessage, ResultMessage, ServiceTarget,
};

/// WebSocket upgrade handler
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| connection::handle_socket(socket, state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_auth_message() {
        let json = r#"{"type": "auth", "access_token": "test_token"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::Auth { access_token, .. } => {
                assert_eq!(access_token, Some("test_token".to_string()));
            }
            _ => panic!("Expected Auth message"),
        }
    }

    #[test]
    fn test_parse_ping_message() {
        let json = r#"{"type": "ping", "id": 1}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::Ping { id } => {
                assert_eq!(id, 1);
            }
            _ => panic!("Expected Ping message"),
        }
    }

    #[test]
    fn test_parse_subscribe_events() {
        let json = r#"{"type": "subscribe_events", "id": 1, "event_type": "state_changed"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::SubscribeEvents { id, event_type } => {
                assert_eq!(id, 1);
                assert_eq!(event_type, Some("state_changed".to_string()));
            }
            _ => panic!("Expected SubscribeEvents message"),
        }
    }

    #[test]
    fn test_parse_call_service() {
        let json = r#"{
            "type": "call_service",
            "id": 1,
            "domain": "light",
            "service": "turn_on",
            "target": {"entity_id": "light.living_room"},
            "service_data": {"brightness": 255}
        }"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::CallService {
                id,
                domain,
                service,
                ..
            } => {
                assert_eq!(id, 1);
                assert_eq!(domain, "light");
                assert_eq!(service, "turn_on");
            }
            _ => panic!("Expected CallService message"),
        }
    }

    #[test]
    fn test_serialize_auth_required() {
        let msg = OutgoingMessage::AuthRequired(AuthRequiredMessage {
            msg_type: "auth_required",
            ha_version: "2024.1.0".to_string(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("auth_required"));
        assert!(json.contains("2024.1.0"));
    }

    #[test]
    fn test_serialize_result() {
        let msg = OutgoingMessage::Result(ResultMessage {
            id: 1,
            msg_type: "result",
            success: true,
            result: Some(serde_json::json!({"test": "value"})),
            error: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"success\":true"));
    }

    #[test]
    fn test_parse_config_entries_subentries_list() {
        let json = r#"{"type": "config_entries/subentries/list", "id": 42, "entry_id": "abc123"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::ConfigEntriesSubentriesList { id, entry_id } => {
                assert_eq!(id, 42);
                assert_eq!(entry_id, "abc123");
            }
            _ => panic!("Expected ConfigEntriesSubentriesList message"),
        }
    }

    #[test]
    fn test_parse_config_entries_get_with_domain() {
        let json = r#"{"type": "config_entries/get", "id": 5, "domain": "sun"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::ConfigEntriesGet {
                id,
                entry_id,
                domain,
            } => {
                assert_eq!(id, 5);
                assert_eq!(entry_id, None);
                assert_eq!(domain, Some("sun".to_string()));
            }
            _ => panic!("Expected ConfigEntriesGet message"),
        }
    }

    #[test]
    fn test_parse_config_entries_get_with_entry_id() {
        let json = r#"{"type": "config_entries/get", "id": 6, "entry_id": "entry123"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::ConfigEntriesGet {
                id,
                entry_id,
                domain,
            } => {
                assert_eq!(id, 6);
                assert_eq!(entry_id, Some("entry123".to_string()));
                assert_eq!(domain, None);
            }
            _ => panic!("Expected ConfigEntriesGet message"),
        }
    }

    #[test]
    fn test_parse_manifest_get() {
        let json = r#"{"type": "manifest/get", "id": 7, "integration": "sun"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::ManifestGet { id, integration } => {
                assert_eq!(id, 7);
                assert_eq!(integration, "sun");
            }
            _ => panic!("Expected ManifestGet message"),
        }
    }

    #[test]
    fn test_parse_manifest_list() {
        let json = r#"{"type": "manifest/list", "id": 8}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::ManifestList { id } => {
                assert_eq!(id, 8);
            }
            _ => panic!("Expected ManifestList message"),
        }
    }
}
