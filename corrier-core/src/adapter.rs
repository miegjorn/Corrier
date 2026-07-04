use crate::message::ChatReply;

/// An agent-facing identity an adapter has provisioned in its own protocol —
/// e.g. a Matrix Application-Service virtual user. The core never inspects
/// the contents of `protocol_handle`; it's opaque outside the adapter that
/// produced it.
#[derive(Debug, Clone)]
pub struct AdapterIdentity {
    pub agent_name: String,
    /// Adapter-specific handle (a Matrix user ID like `@_corrier_guilhem:...`
    /// for the Matrix adapter). Opaque to everything outside the adapter.
    pub protocol_handle: String,
}

/// The contract every protocol adapter implements. Corrièr's core code never
/// matches on which adapter it's talking to — an agent pod, the routing
/// table, and the Nervi wrapper all operate purely in terms of `ChatMessage`/
/// `ChatReply`/`AdapterIdentity`. Matrix is this plan's only implementation;
/// a future Slack/WhatsApp/Teams/IRC adapter implements the same trait.
#[async_trait::async_trait]
pub trait Adapter: Send + Sync {
    /// Short, stable name for this adapter (e.g. "matrix") — matches the
    /// `adapter` field on `ChatMessage`/`ChatReply` and is used as a
    /// namespace prefix wherever adapter identity matters.
    fn name(&self) -> &'static str;

    /// Provision (idempotently) a virtual/managed identity for `agent_name`
    /// in this adapter's protocol. Called once per agent at startup; must be
    /// safe to call repeatedly (already-provisioned is success, not error).
    async fn provision_identity(&self, agent_name: &str) -> anyhow::Result<AdapterIdentity>;

    /// Ensure `agent_name`'s identity is a member of `conversation_id`,
    /// joining it if not. Idempotent — already-a-member must be a no-op
    /// success, not an error. Distinct from `provision_identity`: creating an
    /// identity in the protocol and that identity being present in a
    /// specific conversation are two different facts (a Matrix virtual user
    /// can exist AS-wide but still need to join a given room before it can
    /// post there).
    async fn join_conversation(&self, agent_name: &str, conversation_id: &str) -> anyhow::Result<()>;

    /// Render `reply` for this protocol and deliver it as `agent_name`'s
    /// provisioned identity. The only output side effect this trait defines
    /// — no other method sends anything. Takes `agent_name` explicitly
    /// (rather than putting it on `ChatReply`) because the caller — the
    /// write gateway's per-component drain loop (Task 7) — already has it
    /// in scope from which subject it's consuming; duplicating it onto every
    /// `ChatReply` would be redundant with that.
    async fn deliver(&self, agent_name: &str, reply: &ChatReply) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeAdapter;

    #[async_trait::async_trait]
    impl Adapter for FakeAdapter {
        fn name(&self) -> &'static str {
            "fake"
        }
        async fn provision_identity(&self, agent_name: &str) -> anyhow::Result<AdapterIdentity> {
            Ok(AdapterIdentity {
                agent_name: agent_name.to_string(),
                protocol_handle: format!("fake:{}", agent_name),
            })
        }
        async fn join_conversation(&self, _agent_name: &str, _conversation_id: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn deliver(&self, _agent_name: &str, _reply: &ChatReply) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn fake_adapter_satisfies_the_trait_object_safety() {
        let adapter: Box<dyn Adapter> = Box::new(FakeAdapter);
        assert_eq!(adapter.name(), "fake");
        let identity = adapter.provision_identity("guilhem").await.unwrap();
        assert_eq!(identity.protocol_handle, "fake:guilhem");
    }
}
