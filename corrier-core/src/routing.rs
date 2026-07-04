//! Farga-backed routing: "which Nervi subject(s) does this room's traffic go
//! to." Stored as a Farga context node at path `[<room_id>][routing]`, using
//! Farga's existing generic KV store (`PUT`/`GET /kv/*path`) — same
//! convention `amassada-core::farga` already uses for SessionGraph
//! persistence. Always a *list* of entries, never a single scalar (spec
//! decision 1) — a room with two agent members fans out to two subjects.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteEntry {
    pub component: String,
    pub inbound_subject: String,
}

/// Percent-encode a room ID for use as a URL path segment (Matrix room IDs
/// contain `!` and `:`).
fn encode_path_segment(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '!' | '#' | ':' | '/' | '?' | '&' | '=' | '+' | ' ' | '%' => {
                format!("%{:02X}", c as u32)
            }
            _ => c.to_string(),
        })
        .collect()
}

fn routing_kv_url(farga_url: &str, room_id: &str) -> String {
    format!(
        "{}/kv/routing/{}",
        farga_url.trim_end_matches('/'),
        encode_path_segment(room_id)
    )
}

/// Look up the routing entries for `room_id`. Returns an empty vec (not an
/// error) if none are stored yet — the read gateway treats an empty result
/// as "no agent is listening here," not a fatal condition.
pub async fn resolve_routes(farga_url: &str, room_id: &str) -> anyhow::Result<Vec<RouteEntry>> {
    let url = routing_kv_url(farga_url, room_id);
    let resp = reqwest::Client::new().get(&url).send().await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(vec![]);
    }
    if !resp.status().is_success() {
        anyhow::bail!("farga routing GET {} returned {}", url, resp.status());
    }

    let json: serde_json::Value = resp.json().await?;
    let value = json
        .get("value")
        .ok_or_else(|| anyhow::anyhow!("farga routing GET response has no 'value' field"))?;
    let routes: Vec<RouteEntry> = serde_json::from_value(value.clone())?;
    Ok(routes)
}

/// Store the routing entries for `room_id`, replacing whatever was there.
/// Used by provisioning (Task 5) when an agent joins a room.
pub async fn put_routes(farga_url: &str, room_id: &str, routes: &[RouteEntry]) -> anyhow::Result<()> {
    let url = routing_kv_url(farga_url, room_id);
    let body = serde_json::json!({ "value": routes });
    let resp = reqwest::Client::new().put(&url).json(&body).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("farga routing PUT {} returned {}", url, resp.status());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_path_segment_escapes_matrix_room_id_chars() {
        let encoded = encode_path_segment("!abc123:occitane.guilhem");
        assert!(!encoded.contains('!'));
        assert!(!encoded.contains(':'));
    }

    #[test]
    fn routing_kv_url_is_well_formed() {
        let url = routing_kv_url("http://farga.svc:7500", "!abc:occitane.guilhem");
        assert_eq!(url, "http://farga.svc:7500/kv/routing/%21abc%3Aoccitane.guilhem");
    }

    #[tokio::test]
    async fn resolve_routes_against_unreachable_farga_returns_err_not_panic() {
        let result = resolve_routes("http://127.0.0.1:1", "!x:occitane.guilhem").await;
        assert!(result.is_err());
    }
}
