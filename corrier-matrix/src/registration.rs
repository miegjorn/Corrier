//! Matrix Application Service registration. An AS (not a bot user polling
//! `/sync`) is the primitive Matrix provides specifically for bridge
//! software: Synapse pushes room events to it as HTTP transactions. This
//! module generates the registration YAML Synapse needs, and provisions
//! per-agent virtual users under the claimed namespace -- replacing
//! Caissa's `bootstrap_matrix.rs`, which registered 9 *real* independently-
//! logging-in Matrix accounts. Under this design no agent process ever logs
//! into Matrix; the read/write gateways are the only thing with an AS
//! token, and agent identities are virtual users this AS manages.

use serde::Serialize;

/// The AS's own claimed user-ID namespace prefix. Agent virtual users are
/// `@_corrier_<agent_name>:occitane.guilhem` -- the `_corrier_` prefix marks
/// them as AS-managed to both Synapse (via the namespace regex below) and to
/// a human reading a room member list (visibly distinct from a real account
/// like `@pierre-luc`).
pub const VIRTUAL_USER_PREFIX: &str = "_corrier_";

#[derive(Serialize)]
struct Namespace {
    exclusive: bool,
    regex: String,
}

#[derive(Serialize)]
struct Namespaces {
    users: Vec<Namespace>,
    aliases: Vec<Namespace>,
    rooms: Vec<Namespace>,
}

#[derive(Serialize)]
struct AsRegistration {
    id: String,
    url: String,
    as_token: String,
    hs_token: String,
    sender_localpart: String,
    namespaces: Namespaces,
    rate_limited: bool,
}

/// Generate the registration YAML Synapse loads to recognize this AS.
/// `read_gateway_url` is where Synapse PUTs `/transactions/{txnId}` --
/// the read gateway's own externally-reachable base URL.
pub fn generate_as_registration_yaml(
    as_token: &str,
    hs_token: &str,
    read_gateway_url: &str,
) -> String {
    let reg = AsRegistration {
        id: "corrier".to_string(),
        url: read_gateway_url.to_string(),
        as_token: as_token.to_string(),
        hs_token: hs_token.to_string(),
        sender_localpart: format!("{}bridge", VIRTUAL_USER_PREFIX),
        namespaces: Namespaces {
            users: vec![Namespace {
                exclusive: true,
                regex: format!("@{}.*:occitane\\.guilhem", VIRTUAL_USER_PREFIX),
            }],
            aliases: vec![],
            rooms: vec![],
        },
        rate_limited: false,
    };
    serde_yaml::to_string(&reg).expect("AsRegistration always serializes")
}

/// The virtual user ID for `agent_name` under this AS's namespace.
pub fn virtual_user_id(agent_name: &str) -> String {
    format!("@{}{}:occitane.guilhem", VIRTUAL_USER_PREFIX, agent_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_user_id_uses_the_claimed_prefix() {
        assert_eq!(virtual_user_id("guilhem"), "@_corrier_guilhem:occitane.guilhem");
    }

    #[test]
    fn registration_yaml_contains_exclusive_namespace_regex() {
        let yaml = generate_as_registration_yaml("as-tok", "hs-tok", "http://corrier-read:8080");
        assert!(yaml.contains("exclusive: true"));
        assert!(yaml.contains("_corrier_"));
        assert!(yaml.contains("as-tok"));
        assert!(yaml.contains("hs-tok"));
    }
}
