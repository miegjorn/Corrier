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
    /// Actually register `agent_name`'s virtual user with Synapse via the AS
    /// registration endpoint (`type: m.login.application_service`).
    /// Confirmed live (2026-07-04) that this is required, contrary to this
    /// module's original assumption: Synapse's "lazy creation on first use"
    /// covers ordinary API calls (e.g. sending a message once already a
    /// member), but rejects an impersonated room *join* for a virtual user
    /// that was never actually registered, with "Application service has
    /// not registered this user" — even though the user_id matches the AS's
    /// exclusive namespace regex exactly. Idempotent: `M_USER_IN_USE` (the
    /// response for an already-registered user) is treated as success, not
    /// an error, so this is safe to call on every join/deliver.
    async fn ensure_registered(&self, agent_name: &str) -> anyhow::Result<()> {
        let uid = virtual_user_id(agent_name);
        let localpart = format!("{}{}", crate::registration::VIRTUAL_USER_PREFIX, agent_name);
        let url = format!(
            "{}/_matrix/client/v3/register?user_id={}",
            self.homeserver,
            urlencoding_room_id(&uid),
        );
        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.as_token))
            .json(&serde_json::json!({
                "type": "m.login.application_service",
                "username": localpart,
            }))
            .send()
            .await?;
        if resp.status().is_success() {
            return Ok(());
        }
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        if body.get("errcode").and_then(|v| v.as_str()) == Some("M_USER_IN_USE") {
            return Ok(());
        }
        anyhow::bail!("register {} failed: {} {}", uid, status, body);
    }

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
            let status = resp.status();
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            anyhow::bail!("join {} into {} failed: {} {}", user_id, room_id, status, body);
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
        self.ensure_registered(agent_name).await?;
        Ok(AdapterIdentity {
            agent_name: agent_name.to_string(),
            protocol_handle: virtual_user_id(agent_name),
        })
    }

    async fn join_conversation(&self, agent_name: &str, conversation_id: &str) -> anyhow::Result<()> {
        // Registration is a prerequisite for the join itself (see
        // ensure_registered's doc comment) -- called here, not just left to
        // whatever caller happens to invoke provision_identity first, since
        // join_conversation is reachable directly (handle_provision) without
        // going through provision_identity at all.
        self.ensure_registered(agent_name).await?;
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

    #[test]
    fn virtual_user_id_matches_what_provision_identity_reports() {
        // provision_identity itself now makes a real registration call
        // (ensure_registered) against Synapse -- not unit-testable without a
        // live/mock homeserver, matching this crate's existing convention of
        // not unit-testing network-calling functions (post_reply,
        // matrix_post_body, join_as are all untested at this level too).
        // This just confirms the handle it would report is the same pure
        // computation registration::virtual_user_id already covers.
        assert_eq!(virtual_user_id("guilhem"), "@_corrier_guilhem:occitane.guilhem");
    }
}
