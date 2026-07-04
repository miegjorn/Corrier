//! Matrix message rendering and posting — ported from
//! `Caissa/caissa-cli/src/commands/listen/matrix_client.rs` (pre-Corrièr),
//! with `as_user_id: Option<&str>` threaded through every authenticated call
//! for Matrix Application Service user impersonation (see this module's
//! doc comment above in the plan for why this one extension was necessary).
//! Everything else — markdown-to-HTML, inline mermaid-diagram substitution
//! via Kroki — is unchanged from the original.

fn matrix_pct(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '!' | '#' | '@' | ':' | '/' | '?' | '&' | '=' | '+' | ' ' => format!("%{:02X}", c as u32),
            _ => c.to_string(),
        })
        .collect()
}

/// Appends `?user_id=<as_user_id>` to `url` when `as_user_id` is `Some` —
/// the Matrix Application Service impersonation query parameter. `url` is
/// assumed to have no existing query string (true for every call site below).
fn with_as_user_id(url: String, as_user_id: Option<&str>) -> String {
    match as_user_id {
        Some(uid) => format!("{}?user_id={}", url, matrix_pct(uid)),
        None => url,
    }
}

pub async fn matrix_post_body(homeserver: &str, token: &str, room_id: &str, body: &serde_json::Value, as_user_id: Option<&str>) -> anyhow::Result<()> {
    let txn = uuid::Uuid::new_v4();
    let url = with_as_user_id(
        format!(
            "{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            homeserver, matrix_pct(room_id), txn
        ),
        as_user_id,
    );
    let resp = reqwest::Client::new()
        .put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(body)
        .send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("matrix_post failed: {}", resp.status());
    }
    Ok(())
}

pub fn markdown_body(content: &str) -> serde_json::Value {
    let html = render_markdown(content);
    if html_differs_from_plain(content, &html) {
        serde_json::json!({
            "msgtype": "m.text",
            "body": content,
            "format": "org.matrix.custom.html",
            "formatted_body": html,
        })
    } else {
        serde_json::json!({ "msgtype": "m.text", "body": content })
    }
}

pub fn render_markdown(content: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(content, opts);
    let mut html_out = String::new();
    html::push_html(&mut html_out, parser);
    html_out
}

pub fn html_differs_from_plain(plain: &str, html: &str) -> bool {
    let trimmed = html.trim();
    let unwrapped = trimmed
        .strip_prefix("<p>")
        .and_then(|s| s.strip_suffix("</p>"))
        .unwrap_or(trimmed);
    unwrapped != plain.trim()
}

pub async fn render_diagram_png(kroki_url: &str, diagram: &str) -> anyhow::Result<Vec<u8>> {
    let url = format!("{}/mermaid/png", kroki_url);
    let resp = reqwest::Client::new()
        .post(&url)
        .header("Content-Type", "text/plain")
        .body(diagram.to_string())
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Kroki request failed: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Kroki returned {}: {}", status, body);
    }
    Ok(resp.bytes().await.map(|b| b.to_vec())?)
}

pub async fn matrix_upload_media(homeserver: &str, token: &str, content_type: &str, data: Vec<u8>, as_user_id: Option<&str>) -> anyhow::Result<String> {
    let url = with_as_user_id(format!("{}/_matrix/media/v3/upload", homeserver), as_user_id);
    let resp = reqwest::Client::new()
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", content_type)
        .body(data)
        .send().await?;
    let json: serde_json::Value = resp.json().await?;
    json["content_uri"].as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("upload_media: no content_uri in response: {:?}", json))
}

pub async fn render_and_upload_diagram(homeserver: &str, token: &str, kroki_url: &str, diagram: &str, as_user_id: Option<&str>) -> anyhow::Result<String> {
    let png = render_diagram_png(kroki_url, diagram).await?;
    matrix_upload_media(homeserver, token, "image/png", png, as_user_id).await
}

pub async fn substitute_mermaid_with_images(homeserver: &str, token: &str, kroki_url: &str, content: &str, as_user_id: Option<&str>) -> String {
    let open = "```mermaid";
    let close = "```";
    let mut out = String::new();
    let mut rest = content;
    let mut diagram_num = 0usize;
    while let Some(start) = rest.find(open) {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + open.len()..];
        let body = after_open.trim_start_matches('\n').trim_start_matches('\r');
        if let Some(end) = body.find(close) {
            let diagram = body[..end].trim();
            if !diagram.is_empty() {
                diagram_num += 1;
                match render_and_upload_diagram(homeserver, token, kroki_url, diagram, as_user_id).await {
                    Ok(mxc) => out.push_str(&format!("![diagram {}]({})", diagram_num, mxc)),
                    Err(e) => {
                        tracing::warn!("post_reply: kroki render/upload failed for diagram {} (leaving raw source in place): {}", diagram_num, e);
                        out.push_str(&format!("```mermaid\n{}\n```", diagram));
                    }
                }
            }
            rest = &body[end + close.len()..];
        } else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out
}

pub async fn post_reply(homeserver: &str, token: &str, room_id: &str, kroki_url: &str, reply: &str, as_user_id: Option<&str>) -> anyhow::Result<()> {
    let substituted = substitute_mermaid_with_images(homeserver, token, kroki_url, reply, as_user_id).await;
    if !substituted.trim().is_empty() {
        matrix_post_body(homeserver, token, room_id, &markdown_body(&substituted), as_user_id).await?;
    }
    Ok(())
}

#[cfg(test)]
mod matrix_rendering_tests {
    use super::*;

    #[test]
    fn plain_prose_has_no_formatted_body() {
        let body = markdown_body("hello world");
        assert!(body.get("formatted_body").is_none());
        assert_eq!(body["body"], "hello world");
    }

    #[test]
    fn markdown_with_structure_includes_formatted_body() {
        let body = markdown_body("# Heading\n\n- item one\n- item two");
        assert!(body.get("formatted_body").is_some());
        assert_eq!(body["format"], "org.matrix.custom.html");
    }

    #[test]
    fn html_differs_from_plain_detects_real_markup() {
        assert!(html_differs_from_plain("**bold**", "<p><strong>bold</strong></p>\n"));
    }

    #[test]
    fn html_differs_from_plain_false_for_bare_paragraph_wrap() {
        assert!(!html_differs_from_plain("hello world", "<p>hello world</p>\n"));
    }
}
