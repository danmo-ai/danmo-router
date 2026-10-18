use crate::error::{RelayError, RelayResult};
use crate::state::RelayState;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use danmo_core::billing::{self, BillingSession, Usage};
use danmo_core::services::{channel, log as log_svc, token as token_svc};
use danmo_core::{routing, AppError};
use futures::stream::{self, StreamExt};
use serde_json::Value;
use std::time::Instant;
use tracing::warn;

pub struct RelayRequest {
    pub path: String,
    pub auth_header: String,
    pub body: Bytes,
    pub content_type: Option<String>,
}

pub async fn handle_openai_compat(state: &RelayState, req: RelayRequest) -> RelayResult<Response> {
    let auth = token_svc::authenticate_api_key(&state.pool, &req.auth_header).await?;
    let body_json: Value = serde_json::from_slice(&req.body)
        .map_err(|e| AppError::BadRequest(format!("invalid json body: {e}")))?;

    let model = body_json
        .get("model")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("model is required".into()))?
        .to_string();

    if !auth.token.allows_model(&model) {
        return Err(
            AppError::Forbidden(format!("model '{model}' not allowed for this token")).into(),
        );
    }

    let stream = body_json
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let max_tokens = body_json
        .get("max_tokens")
        .and_then(|v| v.as_i64())
        .or_else(|| {
            body_json
                .get("max_completion_tokens")
                .and_then(|v| v.as_i64())
        });

    let group = token_svc::resolve_group(&auth.token, &auth.user);
    let billing = billing::pre_consume(
        &state.pool,
        &auth.user,
        &auth.token,
        &group,
        &model,
        max_tokens,
    )
    .await?;

    let max_retries = state.config.relay_max_retries.max(1);
    let mut exclude = Vec::new();
    let mut last_error = String::from("no channel available");
    let started = Instant::now();

    for _attempt in 0..max_retries {
        let candidates = match routing::select_channels(&state.pool, &group, &model, &exclude).await
        {
            Ok(c) => c,
            Err(e) => {
                last_error = e.to_string();
                break;
            }
        };

        let Some(candidate) = candidates.into_iter().next() else {
            break;
        };
        let ch = candidate.channel;
        exclude.push(ch.id);

        let upstream_key =
            match channel::decrypt_channel_key(&state.config.encryption_key, &ch).await {
                Ok(k) => k,
                Err(e) => {
                    last_error = e.to_string();
                    continue;
                }
            };

        let url = format!("{}{}", ch.base_url.trim_end_matches('/'), req.path);
        let mut builder = state
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {upstream_key}"))
            .header("Content-Type", "application/json")
            .body(req.body.clone());

        if let Some(ct) = &req.content_type {
            builder = builder.header("Content-Type", ct);
        }

        let upstream = match builder.send().await {
            Ok(r) => r,
            Err(e) => {
                last_error = format!("upstream connect error: {e}");
                warn!(channel_id = ch.id, error = %last_error, "upstream failed");
                continue;
            }
        };

        let status = upstream.status();
        if should_retry(status) {
            last_error = format!("upstream status {}", status.as_u16());
            warn!(
                channel_id = ch.id,
                status = status.as_u16(),
                "retrying channel"
            );
            let _ = upstream.bytes().await;
            continue;
        }

        if stream {
            return stream_response(
                state.clone(),
                upstream,
                billing,
                auth.user.id,
                auth.token.id,
                ch.id,
                model,
                started,
                &req.body,
            )
            .await;
        }

        return buffered_response(
            state,
            upstream,
            billing,
            auth.user.id,
            auth.token.id,
            ch.id,
            model,
            started,
            &req.body,
        )
        .await;
    }

    let _ = billing::refund(&state.pool, &billing).await;
    let _ = log_svc::insert_log(
        &state.pool,
        log_svc::NewLog {
            user_id: auth.user.id,
            token_id: Some(auth.token.id),
            channel_id: exclude.last().copied(),
            model,
            prompt_tokens: 0,
            completion_tokens: 0,
            quota: 0,
            duration_ms: started.elapsed().as_millis() as i64,
            status: "error".into(),
            estimated: false,
            error: Some(last_error.clone()),
        },
    )
    .await;

    Err(AppError::Other(anyhow::anyhow!(last_error)).into())
}

fn should_retry(status: reqwest::StatusCode) -> bool {
    status.as_u16() == 429 || status.is_server_error()
}

#[allow(clippy::too_many_arguments)]
async fn buffered_response(
    state: &RelayState,
    upstream: reqwest::Response,
    billing: BillingSession,
    user_id: i64,
    token_id: i64,
    channel_id: i64,
    model: String,
    started: Instant,
    request_body: &Bytes,
) -> RelayResult<Response> {
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let headers = filter_headers(upstream.headers());
    let body_bytes = upstream
        .bytes()
        .await
        .map_err(|e| AppError::Other(e.into()))?;

    if !status.is_success() {
        let _ = billing::refund(&state.pool, &billing).await;
        let _ = log_svc::insert_log(
            &state.pool,
            log_svc::NewLog {
                user_id,
                token_id: Some(token_id),
                channel_id: Some(channel_id),
                model,
                prompt_tokens: 0,
                completion_tokens: 0,
                quota: 0,
                duration_ms: started.elapsed().as_millis() as i64,
                status: "error".into(),
                estimated: false,
                error: Some(
                    String::from_utf8_lossy(&body_bytes)
                        .chars()
                        .take(500)
                        .collect(),
                ),
            },
        )
        .await;
        return Ok((status, headers, body_bytes).into_response());
    }

    let usage = extract_usage(&body_bytes).unwrap_or_else(|| {
        let prompt_chars = request_body.len();
        let completion_chars = estimate_completion_chars(&body_bytes);
        billing::estimate_usage_from_chars(prompt_chars, completion_chars)
    });

    let quota = billing::settle(&state.pool, &billing, &usage)
        .await
        .unwrap_or(billing.pre_consumed);

    let _ = log_svc::insert_log(
        &state.pool,
        log_svc::NewLog {
            user_id,
            token_id: Some(token_id),
            channel_id: Some(channel_id),
            model,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            quota,
            duration_ms: started.elapsed().as_millis() as i64,
            status: "success".into(),
            estimated: usage.estimated,
            error: None,
        },
    )
    .await;

    Ok((status, headers, body_bytes).into_response())
}

#[allow(clippy::too_many_arguments)]
async fn stream_response(
    state: RelayState,
    upstream: reqwest::Response,
    billing: BillingSession,
    user_id: i64,
    token_id: i64,
    channel_id: i64,
    model: String,
    started: Instant,
    request_body: &Bytes,
) -> RelayResult<Response> {
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let headers = filter_headers(upstream.headers());

    if !status.is_success() {
        let body_bytes = upstream
            .bytes()
            .await
            .map_err(|e| AppError::Other(e.into()))?;
        let _ = billing::refund(&state.pool, &billing).await;
        let _ = log_svc::insert_log(
            &state.pool,
            log_svc::NewLog {
                user_id,
                token_id: Some(token_id),
                channel_id: Some(channel_id),
                model,
                prompt_tokens: 0,
                completion_tokens: 0,
                quota: 0,
                duration_ms: started.elapsed().as_millis() as i64,
                status: "error".into(),
                estimated: false,
                error: Some(
                    String::from_utf8_lossy(&body_bytes)
                        .chars()
                        .take(500)
                        .collect(),
                ),
            },
        )
        .await;
        return Ok((status, headers, body_bytes).into_response());
    }

    let prompt_chars = request_body.len();
    let mut byte_stream = upstream.bytes_stream();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);

    tokio::spawn(async move {
        let mut collected = Vec::new();
        while let Some(chunk) = byte_stream.next().await {
            match chunk {
                Ok(bytes) => {
                    collected.extend_from_slice(&bytes);
                    if tx.send(Ok(bytes)).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(std::io::Error::other(e.to_string()))).await;
                    break;
                }
            }
        }
        drop(tx);

        let usage = extract_usage_from_sse(&collected).unwrap_or_else(|| {
            let completion_chars = collected.len();
            billing::estimate_usage_from_chars(prompt_chars, completion_chars)
        });

        let quota = billing::settle(&state.pool, &billing, &usage)
            .await
            .unwrap_or(billing.pre_consumed);

        let _ = log_svc::insert_log(
            &state.pool,
            log_svc::NewLog {
                user_id,
                token_id: Some(token_id),
                channel_id: Some(channel_id),
                model,
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                quota,
                duration_ms: started.elapsed().as_millis() as i64,
                status: "success".into(),
                estimated: usage.estimated,
                error: None,
            },
        )
        .await;
    });

    let body_stream = stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    });
    let body = Body::from_stream(body_stream);
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    Ok(response)
}

fn filter_headers(upstream: &HeaderMap) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for key in ["content-type", "cache-control"] {
        if let Some(v) = upstream.get(key) {
            if let Ok(name) = axum::http::HeaderName::try_from(key) {
                headers.insert(name, v.clone());
            }
        }
    }
    if !headers.contains_key("content-type") {
        headers.insert("content-type", HeaderValue::from_static("application/json"));
    }
    headers
}

fn extract_usage(body: &[u8]) -> Option<Usage> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let usage = v.get("usage")?;
    Some(Usage {
        prompt_tokens: usage.get("prompt_tokens")?.as_i64()?,
        completion_tokens: usage
            .get("completion_tokens")
            .and_then(|x| x.as_i64())
            .unwrap_or(0),
        estimated: false,
    })
}

fn extract_usage_from_sse(body: &[u8]) -> Option<Usage> {
    let text = String::from_utf8_lossy(body);
    let mut last = None;
    for line in text.lines() {
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(data) {
            if let Some(usage) = v.get("usage") {
                if let (Some(p), c) = (
                    usage.get("prompt_tokens").and_then(|x| x.as_i64()),
                    usage
                        .get("completion_tokens")
                        .and_then(|x| x.as_i64())
                        .unwrap_or(0),
                ) {
                    last = Some(Usage {
                        prompt_tokens: p,
                        completion_tokens: c,
                        estimated: false,
                    });
                }
            }
        }
    }
    last
}

fn estimate_completion_chars(body: &[u8]) -> usize {
    if let Ok(v) = serde_json::from_slice::<Value>(body) {
        if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
            return choices
                .iter()
                .filter_map(|c| {
                    c.pointer("/message/content")
                        .and_then(|x| x.as_str())
                        .map(|s| s.len())
                        .or_else(|| c.pointer("/text").and_then(|x| x.as_str()).map(|s| s.len()))
                })
                .sum();
        }
    }
    body.len() / 4
}

pub async fn list_models(state: &RelayState, auth_header: &str) -> RelayResult<Response> {
    let auth = token_svc::authenticate_api_key(&state.pool, auth_header).await?;
    let group = token_svc::resolve_group(&auth.token, &auth.user);
    let models = channel::list_available_models(&state.pool, &group).await?;
    let data: Vec<Value> = models
        .into_iter()
        .filter(|m| auth.token.allows_model(m))
        .map(|id| {
            serde_json::json!({
                "id": id,
                "object": "model",
                "owned_by": "danmo"
            })
        })
        .collect();

    Ok(axum::Json(serde_json::json!({
        "object": "list",
        "data": data
    }))
    .into_response())
}

// Silence unused import warning for RelayError in some builds
#[allow(dead_code)]
fn _use_relay_error(_: RelayError) {}
