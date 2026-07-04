use crate::registration::virtual_user_id;
use crate::rendering::post_reply;
use corrier_core::{Adapter, AdapterIdentity, ChatReply};

/// The Matrix implementation of `corrier_core::Adapter`. Holds the AS's own
/// as_token (used to act as any virtual user without a per-agent login) and
/// the homeserver/Kroki URLs `post_reply` needs.
pub struct MatrixAdapter {
    pub homeserver: String,
    pub as_token: String,
    pub kroki_url: String,
}

impl MatrixAdapter {
    /// Join `user_id` (an AS virtual user) into `room_id`, impersonating it
    /// via the same `?user_id=` query parameter every other AS call in this
    /// adapter uses. Idempotent — Synapse returns success for an
    /// already-joined room, matching this trait method's own contract.
    async fn join_as(&self, user_id: &str, room_id: &str) -> anyhow::Result<()> {
        let url = format!(
            "{}/_matrix/client/v3/join/{}?user_id={}",
            self.homeserver,
            urlencoding_room_id(room_id),
            urlencoding_room_id(user_id),
        );
        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.as_token))
            .json(&serde_json::json!({}))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("join {} into {} failed: {}", user_id, room_id, resp.status());
        }
        Ok(())
    }
}

fn urlencoding_room_id(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '!' | '#' | '@' | ':' | '/' | '?' | '&' | '=' | '+' | ' ' => format!("%{:02X}", c as u32),
            _ => c.to_string(),
        })
        .collect()
}

#[async_trait::async_trait]
impl Adapter for MatrixAdapter {
    fn name(&self) -> &'static str {
        "matrix"
    }

    async fn provision_identity(&self, agent_name: &str) -> anyhow::Result<AdapterIdentity> {
        // AS virtual users need no separate registration call: Synapse
        // creates them lazily the first time the AS acts as that user ID
        // (using as_token + a `user_id` query param), per the AS spec.
        // Provisioning here is therefore just computing the handle --
        // idempotent by construction, nothing to fail on a repeat call.
        Ok(AdapterIdentity {
            agent_name: agent_name.to_string(),
            protocol_handle: virtual_user_id(agent_name),
        })
    }

    async fn join_conversation(&self, agent_name: &str, conversation_id: &str) -> anyhow::Result<()> {
        self.join_as(&virtual_user_id(agent_name), conversation_id).await
    }

    async fn deliver(&self, agent_name: &str, reply: &ChatReply) -> anyhow::Result<()> {
        // Ensure the virtual user is actually a member of the room before
        // attempting to post -- Synapse rejects a send from a non-member
        // regardless of the AS's own privileges. Idempotent, so this is safe
        // to call on every delivery rather than tracking membership state
        // separately (which would be exactly the kind of gateway-side state
        // this design's Statelessness section rules out).
        self.join_conversation(agent_name, &reply.conversation_id).await?;

        let uid = virtual_user_id(agent_name);
        post_reply(
            &self.homeserver,
            &self.as_token,
            &reply.conversation_id,
            &self.kroki_url,
            &reply.content,
            Some(&uid),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn provision_identity_returns_the_virtual_user_handle() {
        let adapter = MatrixAdapter {
            homeserver: "http://synapse.svc:8008".to_string(),
            as_token: "as-tok".to_string(),
            kroki_url: "http://kroki.svc:8000".to_string(),
        };
        let identity = adapter.provision_identity("guilhem").await.unwrap();
        assert_eq!(identity.protocol_handle, "@_corrier_guilhem:occitane.guilhem");
        assert_eq!(identity.agent_name, "guilhem");
    }
}
