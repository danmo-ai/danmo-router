use danmo_core::protocol::AuthStyle;
use danmo_core::services::channel;
use danmo_core::{AppError, AppResult, Config, DbPool};
use serde_json::{json, Value};
use std::time::Instant;

/// Asks the upstream `{base_url}/models` with the channel's auth style. Does not bill.
pub async fn test_channel(pool: &DbPool, config: &Config, id: i64) -> AppResult<Value> {
    let channel = channel::get_by_id(pool, id).await?;
    let key = channel::decrypt_channel_key(&config.encryption_key, &channel).await?;
    let url = models_url(&channel.base_url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::BadRequest(format!("http client: {e}")))?;
    let mut req = client.get(&url);
    match channel.auth() {
        AuthStyle::Bearer => {
            req = req.bearer_auth(key);
        }
        AuthStyle::XApiKey => {
            req = req.header("x-api-key", key);
        }
        AuthStyle::XGoogApiKey => {
            req = req.header("x-goog-api-key", key);
        }
    }
    let started = Instant::now();
    let result = req.send().await;
    let latency_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let ok = resp.status().is_success();
            let error = if ok {
                None
            } else {
                let text = resp.text().await.unwrap_or_default();
                Some(summarize(&text))
            };
            Ok(json!({
                "ok": ok,
                "status": status,
                "latency_ms": latency_ms,
                "url": url,
                "error": error,
            }))
        }
        Err(err) => Ok(json!({
            "ok": false,
            "status": 0,
            "latency_ms": latency_ms,
            "url": url,
            "error": summarize(&err.to_string()),
        })),
    }
}

fn models_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/models") {
        base.to_string()
    } else {
        format!("{base}/models")
    }
}

fn summarize(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(240).collect()
}
