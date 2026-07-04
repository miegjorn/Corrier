//! Corrièr write gateway: subscribes to Nervi outbound chat subjects for a
//! configured set of components, delivers each reply back through Matrix.
//! Stateless -- its only input is Nervi, by construction it has no way to
//! know a reply exists unless it arrived there (spec's Statelessness
//! section). No per-room map, no model credential beyond the shared AS
//! as_token used for every delivery.

use corrier_core::{consume_outbound, Adapter};
use corrier_matrix::MatrixAdapter;
use futures::StreamExt;
use std::sync::Arc;

fn parse_components(raw: &str) -> Vec<String> {
    raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let components = parse_components(
        &std::env::var("CORRIER_COMPONENTS")
            .map_err(|_| anyhow::anyhow!("CORRIER_COMPONENTS not set"))?,
    );
    let homeserver = std::env::var("MATRIX_HOMESERVER")
        .unwrap_or_else(|_| "http://synapse.occitan-system.svc.cluster.local:8008".into());
    let as_token = std::env::var("CORRIER_AS_TOKEN")
        .map_err(|_| anyhow::anyhow!("CORRIER_AS_TOKEN not set"))?;
    let kroki_url = std::env::var("KROKI_URL")
        .unwrap_or_else(|_| "http://kroki.occitan-system.svc.cluster.local:8000".into());
    let nats_url = std::env::var("NATS_URL")
        .unwrap_or_else(|_| "nats://nats.occitan-system.svc.cluster.local:4222".into());

    let nervi = nervi_core::NerviClient::connect(&nats_url).await?;
    let adapter = Arc::new(MatrixAdapter { homeserver, as_token, kroki_url });

    let mut handles = Vec::new();
    for component in components {
        let nervi = nervi.clone();
        let adapter = Arc::clone(&adapter);
        handles.push(tokio::spawn(async move {
            drain_component(nervi, adapter, component).await;
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }
    Ok(())
}

async fn drain_component(nervi: nervi_core::NerviClient, adapter: Arc<MatrixAdapter>, component: String) {
    let mut stream = match consume_outbound(&nervi, &component).await {
        Ok(s) => Box::pin(s),
        Err(e) => {
            tracing::error!("failed to open outbound consumer for {}: {}", component, e);
            return;
        }
    };
    while let Some(result) = stream.next().await {
        match result {
            Ok(reply) => {
                if let Err(e) = adapter.deliver(&component, &reply).await {
                    tracing::warn!("delivery failed for {} room {}: {}", component, reply.conversation_id, e);
                }
            }
            Err(e) => {
                tracing::warn!("failed to decode outbound message for {} (non-fatal): {}", component, e);
            }
        }
    }
}

#[cfg(test)]
mod parse_components_tests {
    use super::parse_components;

    #[test]
    fn splits_on_comma_and_trims_whitespace() {
        assert_eq!(
            parse_components("guilhem, caissa,amassada"),
            vec!["guilhem".to_string(), "caissa".to_string(), "amassada".to_string()]
        );
    }

    #[test]
    fn empty_string_yields_empty_vec() {
        assert!(parse_components("").is_empty());
    }
}
