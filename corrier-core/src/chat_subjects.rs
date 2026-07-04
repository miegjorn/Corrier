//! Per-room Nervi subject naming, symmetric on both directions (spec
//! decision 2): `occitan.chat.inbound.<component>.<room_short_id>` and
//! `occitan.chat.outbound.<component>.<room_short_id>`. Each component
//! subscribes via a wildcard (`occitan.chat.inbound.<component>.>`) to see
//! every room it's a member of with one subscription, while NATS subject
//! semantics give per-room ordering for free -- no consumer-key convention
//! needed on either side.

use crate::message::{ChatMessage, ChatReply};
use futures::{Stream, StreamExt};
use nervi_core::{client::PublishOptions, NerviClient};

/// Short, subject-safe form of a room ID: strips the leading `!` and the
/// trailing `:homeserver` part (NATS subjects reject `!`, `:`, and spaces).
/// `!abc123:occitane.guilhem` -> `abc123`.
pub fn room_short_id(room_id: &str) -> String {
    room_id
        .trim_start_matches('!')
        .split(':')
        .next()
        .unwrap_or(room_id)
        .to_string()
}

pub fn inbound_subject(component: &str, room_id: &str) -> String {
    format!("occitan.chat.inbound.{}.{}", component, room_short_id(room_id))
}

pub fn outbound_subject(component: &str, room_id: &str) -> String {
    format!("occitan.chat.outbound.{}.{}", component, room_short_id(room_id))
}

/// Publish an inbound chat message to `component`'s per-room subject. Used
/// by the read gateway (Task 6) after resolving routing.
pub async fn publish_inbound(
    client: &NerviClient,
    component: &str,
    msg: &ChatMessage,
) -> anyhow::Result<()> {
    let subject = inbound_subject(component, &msg.conversation_id);
    client
        .publish(PublishOptions {
            subject,
            payload: serde_json::to_string(msg)?,
            qualifier: Some("info".to_string()),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        })
        .await
}

/// Publish an outbound reply to `component`'s per-room subject. Used by an
/// agent pod (Task 10) after producing a reply.
pub async fn publish_outbound(
    client: &NerviClient,
    component: &str,
    reply: &ChatReply,
) -> anyhow::Result<()> {
    let subject = outbound_subject(component, &reply.conversation_id);
    client
        .publish(PublishOptions {
            subject,
            payload: serde_json::to_string(reply)?,
            qualifier: Some("info".to_string()),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        })
        .await
}

/// Continuously consume `component`'s inbound chat traffic across all its
/// rooms (wildcard subject), yielding decoded `ChatMessage`s. Used by each
/// agent pod's new turn loop (Task 10).
pub async fn consume_inbound(
    client: &NerviClient,
    component: &str,
) -> anyhow::Result<impl Stream<Item = anyhow::Result<ChatMessage>>> {
    let wildcard = format!("occitan.chat.inbound.{}.>", component);
    let durable_name = format!("corrier-inbound-{}", component);
    let raw = client.consume_durable(&wildcard, &durable_name).await?;
    Ok(raw.map(|result| {
        let msg = result?;
        let decoded: ChatMessage = serde_json::from_str(&msg.payload)?;
        Ok(decoded)
    }))
}

/// Continuously consume `component`'s outbound chat traffic across all its
/// rooms. Used by the write gateway (Task 7).
pub async fn consume_outbound(
    client: &NerviClient,
    component: &str,
) -> anyhow::Result<impl Stream<Item = anyhow::Result<ChatReply>>> {
    let wildcard = format!("occitan.chat.outbound.{}.>", component);
    let durable_name = format!("corrier-outbound-{}", component);
    let raw = client.consume_durable(&wildcard, &durable_name).await?;
    Ok(raw.map(|result| {
        let msg = result?;
        let decoded: ChatReply = serde_json::from_str(&msg.payload)?;
        Ok(decoded)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_short_id_strips_bang_and_homeserver() {
        assert_eq!(room_short_id("!abc123:occitane.guilhem"), "abc123");
    }

    #[test]
    fn room_short_id_handles_already_short_input() {
        assert_eq!(room_short_id("abc123"), "abc123");
    }

    #[test]
    fn inbound_and_outbound_subjects_are_symmetric() {
        let room = "!abc123:occitane.guilhem";
        assert_eq!(inbound_subject("guilhem", room), "occitan.chat.inbound.guilhem.abc123");
        assert_eq!(outbound_subject("guilhem", room), "occitan.chat.outbound.guilhem.abc123");
    }
}
