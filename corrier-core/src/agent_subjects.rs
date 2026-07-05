//! Agent-to-agent Nervi subject naming and the message envelope every
//! agent pod's perceive loop routes on. This is the mechanical layer the
//! `occitan/amassada` skill's judgment content depends on -- deterministic
//! code, not left to an LLM's interpretation each time (see
//! docs/superpowers/specs/2026-07-05-amassada-as-agent-skill-design.md,
//! "Amassada's fate: skill, not service"). Lives alongside
//! `chat_subjects.rs`'s existing per-room naming convention, in the same
//! crate every agent pod already depends on.

use serde::{Deserialize, Serialize};

/// Subject a component's persistent perceive loop watches for dispatch
/// orders addressed to it (unchanged from the subject Guilhem's dream/
/// sre-alert prompts already publish to -- this module only adds a typed
/// Rust wrapper around a name that already exists on the wire).
pub fn dispatch_subject(component: &str) -> String {
    format!("occitan.dispatch.{}", component)
}

/// Subject a periodic-maintenance skill's self-scheduled tick arrives on.
/// One subject per (component, skill) pair, e.g. `occitan.tick.guilhem.dream`.
pub fn tick_subject(component: &str, skill: &str) -> String {
    format!("occitan.tick.{}.{}", component, skill)
}

/// The single, already-existing reactive alert subject the SRE watchdog
/// publishes to (`caissa-cli/src/commands/watch.rs::publish_sre_alert`).
/// Named here so every consumer (Task 3's perceive loop) shares one
/// constant instead of a string literal.
pub const SRE_ALERT_SUBJECT: &str = "occitan.sre.alerts";

/// Mint the request/reply subject pair for one dispatcher assignment.
/// `assignment_id` must be unique per `invoke_agent` call (Task 5 mints it
/// from a UUID, matching `dispatch.rs::create_agent_job`'s existing
/// `short_id` convention). The dispatched specialist's Job perceives
/// `request`; the caller (whoever ran `invoke_agent`) perceives `reply`.
pub fn mint_assignment_subjects(assignment_id: &str) -> (String, String) {
    (
        format!("occitan.assignment.{}.request", assignment_id),
        format!("occitan.assignment.{}.reply", assignment_id),
    )
}

/// One message arriving on an agent's perceive queue, tagged so the
/// receiving perceive-loop knows which skill/handler to invoke without
/// inspecting field shape. `serde(tag = "type")` gives every variant an
/// explicit `"type": "..."` field on the wire -- the discriminant Task 3's
/// dispatch-by-type routing switches on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PerceivedMessage {
    /// A task order from another agent (today: Guilhem's dream/sre-alert
    /// prompts publishing to `occitan.dispatch.<component>`).
    Dispatch {
        task: String,
        dispatched_by: String,
        risk_class: u8,
    },
    /// A self-scheduled periodic-maintenance wake (Task 2's `schedule_tick`).
    Tick { skill: String },
    /// A reactive anomaly push from the SRE watchdog (Task 4).
    SreAlert { anomalies: Vec<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_subject_is_stable() {
        assert_eq!(dispatch_subject("farga"), "occitan.dispatch.farga");
    }

    #[test]
    fn tick_subject_includes_skill_name() {
        assert_eq!(tick_subject("guilhem", "dream"), "occitan.tick.guilhem.dream");
    }

    #[test]
    fn assignment_subjects_are_a_distinct_request_reply_pair() {
        let (req, reply) = mint_assignment_subjects("assign-abc123");
        assert_eq!(req, "occitan.assignment.assign-abc123.request");
        assert_eq!(reply, "occitan.assignment.assign-abc123.reply");
        assert_ne!(req, reply);
    }

    #[test]
    fn perceived_message_round_trips_dispatch_variant_through_json() {
        let msg = PerceivedMessage::Dispatch {
            task: "investigate flaky test".to_string(),
            dispatched_by: "guilhem".to_string(),
            risk_class: 1,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: PerceivedMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn perceived_message_round_trips_tick_variant_through_json() {
        let msg = PerceivedMessage::Tick { skill: "dream".to_string() };
        let json = serde_json::to_string(&msg).unwrap();
        let back: PerceivedMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn perceived_message_round_trips_sre_alert_variant_through_json() {
        let msg = PerceivedMessage::SreAlert {
            anomalies: vec!["build failure in Farga".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: PerceivedMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn perceived_message_tag_field_is_a_type_discriminant() {
        // The envelope's wire shape must carry an explicit "type" tag so a
        // receiver's perceive-loop can route to the right skill without
        // guessing from field shape alone (spec: "a message with a type
        // field arriving on the agent's own perceive queue").
        let msg = PerceivedMessage::Tick { skill: "chronicle".to_string() };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "tick");
    }
}
