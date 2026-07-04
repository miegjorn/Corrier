//! Parse a Matrix Application Service transaction (the HTTP body Synapse
//! PUTs to `/_matrix/app/v1/transactions/{txnId}`) into canonical
//! `ChatMessage`s. No interpretation beyond "is this a real text message
//! from a real user, not this AS's own virtual users echoing" -- routing,
//! session lookup, and everything else happens after this, in the read
//! gateway (Task 6).

use corrier_core::ChatMessage;

/// Extract every `m.room.message` event from a transaction body's `events`
/// array into canonical `ChatMessage`s. `own_prefix` is this AS's virtual
/// user prefix (`registration::VIRTUAL_USER_PREFIX`) -- messages sent by any
/// `@<own_prefix>...` sender are skipped as echoes of an agent's own reply,
/// mirroring the existing `sender == own_user_id` echo guard in Caissa's
/// `run_matrix_client_loop`.
pub fn parse_transaction_events(body: &serde_json::Value, own_prefix: &str) -> Vec<ChatMessage> {
    let mut out = Vec::new();
    let Some(events) = body["events"].as_array() else {
        return out;
    };
    for ev in events {
        if ev["type"].as_str() != Some("m.room.message") {
            continue;
        }
        let sender = ev["sender"].as_str().unwrap_or_default().to_string();
        if sender.starts_with(&format!("@{}", own_prefix)) {
            continue;
        }
        let content = ev["content"]["body"].as_str().unwrap_or_default().to_string();
        let room_id = ev["room_id"].as_str().unwrap_or_default().to_string();
        let event_id = ev["event_id"].as_str().map(|s| s.to_string());
        if content.is_empty() || room_id.is_empty() {
            continue;
        }
        out.push(ChatMessage {
            conversation_id: room_id,
            sender,
            content,
            adapter: "matrix".to_string(),
            external_event_id: event_id,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_transaction(sender: &str) -> serde_json::Value {
        serde_json::json!({
            "events": [{
                "type": "m.room.message",
                "sender": sender,
                "room_id": "!abc123:occitane.guilhem",
                "event_id": "$xyz",
                "content": { "body": "hello guilhem", "msgtype": "m.text" }
            }]
        })
    }

    #[test]
    fn parses_a_real_user_message() {
        let msgs = parse_transaction_events(&sample_transaction("@pierre-luc:occitane.guilhem"), "_corrier_");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sender, "@pierre-luc:occitane.guilhem");
        assert_eq!(msgs[0].content, "hello guilhem");
        assert_eq!(msgs[0].conversation_id, "!abc123:occitane.guilhem");
        assert_eq!(msgs[0].external_event_id, Some("$xyz".to_string()));
    }

    #[test]
    fn skips_own_virtual_user_echo() {
        let msgs = parse_transaction_events(&sample_transaction("@_corrier_guilhem:occitane.guilhem"), "_corrier_");
        assert!(msgs.is_empty());
    }

    #[test]
    fn ignores_non_message_events() {
        let body = serde_json::json!({
            "events": [{ "type": "m.room.member", "sender": "@pierre-luc:occitane.guilhem", "room_id": "!x", "content": {} }]
        });
        let msgs = parse_transaction_events(&body, "_corrier_");
        assert!(msgs.is_empty());
    }

    #[test]
    fn empty_events_array_returns_empty_vec() {
        let msgs = parse_transaction_events(&serde_json::json!({"events": []}), "_corrier_");
        assert!(msgs.is_empty());
    }
}
