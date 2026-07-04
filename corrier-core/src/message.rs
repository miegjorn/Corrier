use serde::{Deserialize, Serialize};

/// A message received from a human, in canonical form — no adapter-specific
/// fields survive past this point. `conversation_id` is the adapter-agnostic
/// handle for "where this came from" (a Matrix room ID today).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub conversation_id: String,
    pub sender: String,
    pub content: String,
    /// Which adapter produced this (e.g. "matrix") — carried through so a
    /// reply can be routed back to the right adapter without re-deriving it.
    pub adapter: String,
    /// The adapter's own event identifier, if it has one (Matrix event ID).
    /// Not used for routing or ordering — purely diagnostic.
    pub external_event_id: Option<String>,
}

/// A reply produced by an agent, addressed back to a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatReply {
    pub conversation_id: String,
    pub content: String,
    pub adapter: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_message_round_trips_through_json() {
        let msg = ChatMessage {
            conversation_id: "!abc:occitane.guilhem".to_string(),
            sender: "@pierre-luc:occitane.guilhem".to_string(),
            content: "hello".to_string(),
            adapter: "matrix".to_string(),
            external_event_id: Some("$xyz".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn chat_reply_round_trips_through_json() {
        let reply = ChatReply {
            conversation_id: "!abc:occitane.guilhem".to_string(),
            content: "reply text".to_string(),
            adapter: "matrix".to_string(),
        };
        let json = serde_json::to_string(&reply).unwrap();
        let back: ChatReply = serde_json::from_str(&json).unwrap();
        assert_eq!(reply, back);
    }
}
