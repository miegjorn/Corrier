//! Corrièr read gateway: registers as a Matrix Application Service. Synapse
//! pushes room events here as HTTP transactions -- replaces
//! `run_matrix_client_loop`'s `/sync` long-poll entirely. Stateless: the
//! routing decision (room_id -> subject list) is a pure function of Farga
//! lookup results, so any replica can handle any transaction with no
//! sticky-routing requirement (spec's Statelessness section).

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{post, put},
    Json, Router,
};
use corrier_core::{put_routes, publish_inbound, resolve_routes, Adapter, RouteEntry};
use corrier_matrix::inbound::parse_transaction_events;
use corrier_matrix::registration::VIRTUAL_USER_PREFIX;
use corrier_matrix::MatrixAdapter;
use std::sync::Arc;

struct GatewayState {
    hs_token: String,
    farga_url: String,
    nervi: nervi_core::NerviClient,
    adapter: MatrixAdapter,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let hs_token = std::env::var("CORRIER_HS_TOKEN")
        .map_err(|_| anyhow::anyhow!("CORRIER_HS_TOKEN not set"))?;
    let farga_url = std::env::var("FARGA_URL")
        .unwrap_or_else(|_| "http://farga.occitan-system.svc.cluster.local:7500".into());
    let nats_url = std::env::var("NATS_URL")
        .unwrap_or_else(|_| "nats://nervi-nats.occitan-system.svc.cluster.local:4222".into());
    let homeserver = std::env::var("MATRIX_HOMESERVER")
        .unwrap_or_else(|_| "http://synapse.occitan-system.svc.cluster.local:8008".into());
    let as_token = std::env::var("CORRIER_AS_TOKEN")
        .map_err(|_| anyhow::anyhow!("CORRIER_AS_TOKEN not set"))?;
    let kroki_url = std::env::var("KROKI_URL")
        .unwrap_or_else(|_| "http://kroki.occitan-system.svc.cluster.local:8000".into());

    let nervi = nervi_core::NerviClient::connect(&nats_url).await?;
    let adapter = MatrixAdapter { homeserver, as_token, kroki_url };

    let state = Arc::new(GatewayState { hs_token, farga_url, nervi, adapter });

    let app = Router::new()
        .route("/_matrix/app/v1/transactions/:txn_id", put(handle_transaction))
        .route("/provision", post(handle_provision))
        .route("/health", axum::routing::get(|| async { "ok" }))
        .with_state(state);

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("corrier-read-gateway listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(serde::Deserialize)]
struct ProvisionRequest {
    room_id: String,
    components: Vec<String>,
}

/// Add one or more components to a room's routing: joins each component's
/// virtual user into the room (so the AS actually starts receiving events
/// for it -- Synapse only pushes transactions for rooms an AS-namespaced
/// user is a member of) and writes the resulting route list to Farga.
/// Replaces bootstrap_matrix.rs's invite+join job (Task 9), generalized from
/// "9 static rooms set up once" to "any room, any component, at any time" --
/// this is the mechanism that resolves the read gateway's own bootstrapping
/// requirement: it cannot receive events from a room until something has
/// called this at least once for it.
async fn handle_provision(
    State(state): State<Arc<GatewayState>>,
    Json(req): Json<ProvisionRequest>,
) -> StatusCode {
    for component in &req.components {
        if let Err(e) = state.adapter.join_conversation(component, &req.room_id).await {
            tracing::warn!("provision: join_conversation for {} into {} failed: {}", component, req.room_id, e);
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    }

    let routes: Vec<RouteEntry> = req
        .components
        .iter()
        .map(|c| RouteEntry {
            component: c.clone(),
            inbound_subject: corrier_core::inbound_subject(c, &req.room_id),
        })
        .collect();

    match put_routes(&state.farga_url, &req.room_id, &routes).await {
        Ok(()) => StatusCode::OK,
        Err(e) => {
            tracing::warn!("provision: put_routes for {} failed: {}", req.room_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

fn verify_hs_token(headers: &HeaderMap, configured: &str) -> bool {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim_start_matches("Bearer ").trim())
        == Some(configured)
}

async fn handle_transaction(
    State(state): State<Arc<GatewayState>>,
    Path(_txn_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> StatusCode {
    if !verify_hs_token(&headers, &state.hs_token) {
        tracing::warn!("corrier-read-gateway: transaction with invalid hs_token rejected");
        return StatusCode::FORBIDDEN;
    }

    let messages = parse_transaction_events(&body, VIRTUAL_USER_PREFIX);
    for msg in messages {
        let routes = match resolve_routes(&state.farga_url, &msg.conversation_id).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("routing lookup failed for {}: {} (message dropped)", msg.conversation_id, e);
                continue;
            }
        };
        if routes.is_empty() {
            tracing::debug!("no routes for room {} -- no agent listening here", msg.conversation_id);
            continue;
        }
        for route in routes {
            if let Err(e) = publish_inbound(&state.nervi, &route.component, &msg).await {
                tracing::warn!("publish_inbound to {} failed: {}", route.component, e);
            }
        }
    }

    // Synapse requires a 200 with an empty JSON object to consider the
    // transaction delivered; anything else is retried.
    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn verify_hs_token_accepts_matching_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", HeaderValue::from_static("Bearer correct-token"));
        assert!(verify_hs_token(&headers, "correct-token"));
    }

    #[test]
    fn verify_hs_token_rejects_wrong_token() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", HeaderValue::from_static("Bearer wrong-token"));
        assert!(!verify_hs_token(&headers, "correct-token"));
    }

    #[test]
    fn verify_hs_token_rejects_missing_header() {
        let headers = HeaderMap::new();
        assert!(!verify_hs_token(&headers, "correct-token"));
    }
}
