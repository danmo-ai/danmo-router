//! One pipeline for every inbound protocol.
//!
//! The client speaks the inbound protocol in the model's native dialect (`standard` unless the
//! price configures one); the upstream speaks the channel's protocol in the dialect of the
//! model's row. When both sides match, or the row is opaque, the request is a passthrough: the
//! bytes cross unchanged except for three edits (`model` mapping, Chat stream usage injection,
//! and usage re-rendered into the native dialect's field set). Any other pairing is converted
//! through the IR. Usage is always read from the raw upstream response and matched by field
//! features; a response without billable usage is a billing failure.

use crate::collect::{self, UsageCollector};
use crate::convert::{self, StreamConverter};
use crate::dialect::{self, DialectCodec};
use crate::error::{protocol_error, upstream_message, RelayResult};
use crate::health;
use crate::sse::{self, SseEvent, SseParser};
use crate::state::RelayState;
use crate::upstream;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use danmo_core::billing::{self, BillingSession};
use danmo_core::dialect::Dialect;
use danmo_core::models::Channel;
use danmo_core::protocol::{Protocol, RelayMode};
use danmo_core::services::{channel, log as log_svc, token as token_svc};
use danmo_core::usage::{match_usage, Usage};
use danmo_core::{routing, AppError};
use futures::stream::{self, StreamExt};
use serde_json::{json, Value};
use std::time::Instant;
use tracing::warn;

pub struct Inbound {
    pub protocol: Protocol,
    pub api_key: String,
    pub headers: HeaderMap,
    pub body: Bytes,
    /// Gemini carries the model and the streaming flag in the path.
    pub path_model: Option<String>,
    pub path_stream: bool,
}

struct Parsed {
    json: Value,
    model: String,
    stream: bool,
    max_tokens: Option<i64>,
}

fn parse_inbound(inbound: &Inbound) -> Result<Parsed, AppError> {
    let json: Value = serde_json::from_slice(&inbound.body)
        .map_err(|e| AppError::BadRequest(format!("invalid json body: {e}")))?;
    if !json.is_object() {
        return Err(AppError::BadRequest("body must be a JSON object".into()));
    }
    let model = match &inbound.path_model {
        Some(m) => m.clone(),
        None => json
            .get("model")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::BadRequest("model is required".into()))?
            .to_string(),
    };
    let stream = match inbound.protocol {
        Protocol::Gemini => inbound.path_stream,
        Protocol::Embeddings => false,
        _ => json.get("stream").and_then(Value::as_bool).unwrap_or(false),
    };
    let max_tokens = [
        "/max_tokens",
        "/max_completion_tokens",
        "/max_output_tokens",
        "/generationConfig/maxOutputTokens",
    ]
    .iter()
    .find_map(|p| json.pointer(p).and_then(Value::as_i64));
    Ok(Parsed {
        json,
        model,
        stream,
        max_tokens,
    })
}

/// Request body for a passthrough channel: original bytes unless an allowed edit applies.
/// Returns the body and whether `include_usage` was injected on the client's behalf.
fn passthrough_body(
    inbound: &Inbound,
    parsed: &Parsed,
    ch: &Channel,
    upstream_model: &str,
) -> (Bytes, bool) {
    let remap = inbound.protocol != Protocol::Gemini && upstream_model != parsed.model;
    let inject = inbound.protocol == Protocol::OpenaiChat
        && parsed.stream
        && ch.injects_stream_usage()
        && parsed.json.pointer("/stream_options/include_usage") != Some(&Value::Bool(true));
    if !remap && !inject {
        return (inbound.body.clone(), false);
    }
    let mut body = parsed.json.clone();
    if remap {
        body["model"] = json!(upstream_model);
    }
    if inject {
        match body
            .get_mut("stream_options")
            .and_then(Value::as_object_mut)
        {
            Some(opts) => {
                opts.insert("include_usage".into(), json!(true));
            }
            None => body["stream_options"] = json!({"include_usage": true}),
        }
    }
    (Bytes::from(body.to_string()), inject)
}

struct Ctx {
    state: RelayState,
    billing: BillingSession,
    channel_id: i64,
    started: Instant,
    inbound: Protocol,
    upstream: Protocol,
    mode: RelayMode,
    /// Strip the Chat usage chunk that only exists because we injected `include_usage`.
    strip_injected_usage: bool,
    client: &'static dyn DialectCodec,
    upstream_dialect: &'static dyn DialectCodec,
    /// Passthrough only: the configured native dialect, whose usage field set the client sees.
    native: Option<&'static Dialect>,
    /// Chat clients only receive a usage chunk when they asked for it.
    client_wants_usage: bool,
}

pub async fn handle(state: &RelayState, inbound: Inbound) -> RelayResult<Response> {
    let auth = token_svc::authenticate_api_key(&state.pool, &inbound.api_key).await?;
    let parsed = parse_inbound(&inbound)?;
    let model = parsed.model.clone();

    if !auth.token.allows_model(&model) {
        return Err(
            AppError::Forbidden(format!("model '{model}' not allowed for this token")).into(),
        );
    }

    let group = token_svc::resolve_group(&auth.token, &auth.user);
    let billing = billing::pre_consume(
        &state.pool,
        &auth.user,
        &auth.token,
        &group,
        &model,
        parsed.max_tokens,
    )
    .await?;
    let native = billing.native_dialects.configured(inbound.protocol);
    let in_dialect = billing.native_dialects.get(inbound.protocol);

    let max_retries = state.config.relay_max_retries.max(1);
    let mut exclude = Vec::new();
    let mut last_error = String::from("no channel available");
    let started = Instant::now();

    for _attempt in 0..max_retries {
        let candidates = match routing::eligible_channels(
            &state.pool,
            &group,
            &model,
            inbound.protocol,
            &exclude,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                last_error = e.to_string();
                break;
            }
        };
        let Some(candidate) = state.health.pick(candidates, &model, Instant::now()) else {
            break;
        };
        let up_dialect = candidate.dialect();
        let upstream_model = candidate.upstream_model().to_string();
        let ch = candidate.channel;
        exclude.push(ch.id);

        let key = match channel::decrypt_channel_key(&state.config.encryption_key, &ch).await {
            Ok(k) => k,
            Err(e) => {
                last_error = e.to_string();
                continue;
            }
        };

        let upstream_protocol = ch.protocol();
        let mode = if upstream_protocol == inbound.protocol
            && (up_dialect.opaque || up_dialect.name == in_dialect.name)
        {
            RelayMode::Passthrough
        } else {
            RelayMode::Convert
        };
        let client = dialect::codec(in_dialect);
        let upstream_dialect = dialect::codec(up_dialect);
        let (body, strip_injected_usage) = match mode {
            RelayMode::Passthrough => passthrough_body(&inbound, &parsed, &ch, &upstream_model),
            RelayMode::Convert => {
                match convert::request(
                    client,
                    upstream_dialect,
                    &parsed.json,
                    &upstream_model,
                    parsed.stream,
                ) {
                    Ok(v) => (Bytes::from(v.to_string()), false),
                    Err(e) => {
                        last_error = e;
                        continue;
                    }
                }
            }
        };

        let url = upstream::url(
            &ch.base_url,
            upstream_protocol,
            &upstream_model,
            parsed.stream,
        );
        let headers =
            upstream::request_headers(&inbound.headers, upstream_protocol, ch.auth(), &key);
        let response = match state
            .http
            .post(&url)
            .headers(headers)
            .body(body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_error = format!("upstream connect error: {e}");
                warn!(channel_id = ch.id, error = %last_error, "upstream failed");
                state.health.record_failure(ch.id, &model, Instant::now());
                continue;
            }
        };

        let status = response.status();
        if status.as_u16() == 429 || status.is_server_error() {
            if status.as_u16() == 429 {
                let wait = health::retry_after(response.headers());
                state
                    .health
                    .record_rate_limited(ch.id, &model, wait, Instant::now());
            } else {
                state.health.record_failure(ch.id, &model, Instant::now());
            }
            last_error = format!("upstream status {}", status.as_u16());
            warn!(
                channel_id = ch.id,
                status = status.as_u16(),
                "retrying channel"
            );
            let _ = response.bytes().await;
            continue;
        }

        let ctx = Ctx {
            state: state.clone(),
            billing,
            channel_id: ch.id,
            started,
            inbound: inbound.protocol,
            upstream: upstream_protocol,
            mode,
            strip_injected_usage,
            client,
            upstream_dialect,
            native,
            client_wants_usage: parsed.json.pointer("/stream_options/include_usage")
                == Some(&Value::Bool(true)),
        };

        if !status.is_success() {
            return upstream_error(ctx, response).await;
        }
        if parsed.stream {
            return Ok(stream_response(ctx, response));
        }
        return buffered_response(ctx, response).await;
    }

    let _ = billing::refund(&state.pool, &billing).await;
    let _ = log_svc::insert_log(
        &state.pool,
        log_svc::NewLog {
            channel_id: exclude.last().copied(),
            duration_ms: started.elapsed().as_millis() as i64,
            status: "error".into(),
            error: Some(last_error.clone()),
            ..log_svc::NewLog::new(auth.user.id, Some(auth.token.id), &model)
        },
    )
    .await;

    Err(AppError::Other(anyhow::anyhow!(last_error)).into())
}

impl Ctx {
    /// A reachable upstream counts as healthy even when it rejects the request (4xx); only a
    /// response that breaks off counts against the channel.
    fn report_health(&self, ok: bool) {
        let health = &self.state.health;
        let now = Instant::now();
        if ok {
            health.record_success(self.channel_id, &self.billing.model, now);
        } else {
            health.record_failure(self.channel_id, &self.billing.model, now);
        }
    }
}

fn base_log(ctx: &Ctx, status: &str) -> log_svc::NewLog {
    log_svc::NewLog {
        channel_id: Some(ctx.channel_id),
        duration_ms: ctx.started.elapsed().as_millis() as i64,
        status: status.into(),
        ..log_svc::NewLog::new(
            ctx.billing.user_id,
            Some(ctx.billing.token_id),
            &ctx.billing.model,
        )
    }
}

async fn upstream_error(ctx: Ctx, response: reqwest::Response) -> RelayResult<Response> {
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let headers = upstream::response_headers(response.headers());
    let body = response.bytes().await;
    ctx.report_health(body.is_ok());
    let body = body.map_err(|e| AppError::Other(e.into()))?;
    let message = upstream_message(&body);
    let _ = billing::refund(&ctx.state.pool, &ctx.billing).await;
    let _ = log_svc::insert_log(
        &ctx.state.pool,
        log_svc::NewLog {
            error: Some(message.clone()),
            ..base_log(&ctx, "error")
        },
    )
    .await;
    Ok(match ctx.mode {
        RelayMode::Passthrough => (status, headers, body).into_response(),
        RelayMode::Convert => protocol_error(ctx.inbound, status, &message, "upstream_error"),
    })
}

struct BillingFailure {
    reason: String,
    raw_usage: Option<Value>,
}

fn usage_from_raw(
    raw: Option<Value>,
    protocol: Protocol,
) -> Result<(Usage, Value), BillingFailure> {
    let raw = raw.ok_or_else(|| BillingFailure {
        reason: "upstream response carried no usage".into(),
        raw_usage: None,
    })?;
    match match_usage(&raw, protocol) {
        Ok(u) => Ok((u, raw)),
        Err(e) => Err(BillingFailure {
            reason: e.0,
            raw_usage: Some(raw),
        }),
    }
}

/// Settle on success; on billing failure refund the pre-consumed quota and record why.
async fn finalize(
    ctx: &Ctx,
    outcome: &Result<(Usage, Value), BillingFailure>,
    note: Option<String>,
) {
    match outcome {
        Ok((usage, raw)) => {
            let quota = billing::settle(&ctx.state.pool, &ctx.billing, usage)
                .await
                .unwrap_or(ctx.billing.pre_consumed);
            let _ = log_svc::insert_log(
                &ctx.state.pool,
                log_svc::NewLog {
                    quota,
                    raw_usage: Some(raw.to_string()),
                    error: note,
                    ..base_log(ctx, "success").with_usage(usage)
                },
            )
            .await;
        }
        Err(failure) => {
            let _ = billing::refund(&ctx.state.pool, &ctx.billing).await;
            let reason = match note {
                Some(n) => format!("{}; {n}", failure.reason),
                None => failure.reason.clone(),
            };
            warn!(channel_id = ctx.channel_id, %reason, "billing failed");
            let _ = log_svc::insert_log(
                &ctx.state.pool,
                log_svc::NewLog {
                    raw_usage: failure.raw_usage.as_ref().map(Value::to_string),
                    error: Some(reason),
                    ..base_log(ctx, "billing_failed")
                },
            )
            .await;
        }
    }
}

async fn buffered_response(ctx: Ctx, response: reqwest::Response) -> RelayResult<Response> {
    let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::OK);
    let mut headers = upstream::response_headers(response.headers());
    let bytes = response.bytes().await;
    ctx.report_health(bytes.is_ok());
    let bytes = bytes.map_err(|e| AppError::Other(e.into()))?;

    let parsed: Option<Value> = serde_json::from_slice(&bytes).ok();
    let outcome = match &parsed {
        None => Err(BillingFailure {
            reason: "upstream body is not JSON".into(),
            raw_usage: None,
        }),
        Some(v) => match ctx.upstream_dialect.soft_error(v) {
            Some(reason) => Err(BillingFailure {
                reason,
                raw_usage: None,
            }),
            None => usage_from_raw(collect::from_body(ctx.upstream, v), ctx.upstream),
        },
    };

    let (usage, _) = match &outcome {
        Ok(ok) => ok,
        Err(failure) => {
            finalize(&ctx, &outcome, None).await;
            return Ok(protocol_error(
                ctx.inbound,
                StatusCode::BAD_GATEWAY,
                &format!("billing failed: {}", failure.reason),
                "billing_failed",
            ));
        }
    };
    let body = parsed.expect("usage implies a parsed body");

    let out = match ctx.mode {
        RelayMode::Passthrough => match native_render(ctx.native, usage) {
            Some(rendered) => {
                let mut body = body;
                body["usage"] = rendered;
                Bytes::from(body.to_string())
            }
            None => bytes,
        },
        RelayMode::Convert => {
            match convert::response(
                ctx.upstream_dialect,
                ctx.client,
                &body,
                usage,
                &ctx.billing.model,
            ) {
                Ok(v) => {
                    headers = json_headers();
                    Bytes::from(v.to_string())
                }
                Err(e) => {
                    let failure: Result<(Usage, Value), BillingFailure> = Err(BillingFailure {
                        reason: format!("response conversion failed: {e}"),
                        raw_usage: outcome.as_ref().ok().map(|(_, raw)| raw.clone()),
                    });
                    finalize(&ctx, &failure, None).await;
                    return Ok(protocol_error(
                        ctx.inbound,
                        StatusCode::BAD_GATEWAY,
                        &e,
                        "conversion_failed",
                    ));
                }
            }
        }
    };

    finalize(&ctx, &outcome, None).await;
    Ok((status, headers, out).into_response())
}

/// Usage in the native dialect's field set, when it differs from what the upstream sent.
fn native_render(native: Option<&'static Dialect>, usage: &Usage) -> Option<Value> {
    let render = native?.render_for(&usage.adapter);
    (render.name() != usage.adapter).then(|| render.render(usage))
}

fn json_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("content-type", HeaderValue::from_static("application/json"));
    h
}

fn sse_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        "content-type",
        HeaderValue::from_static("text/event-stream"),
    );
    h.insert("cache-control", HeaderValue::from_static("no-cache"));
    h
}

/// Passthrough stream: forward each event's original bytes, except the injected usage chunk
/// (dropped) and a usage chunk in a non-native field set (re-rendered).
struct PassthroughStream {
    protocol: Protocol,
    strip_injected_usage: bool,
    native: Option<&'static Dialect>,
}

impl PassthroughStream {
    fn event(&mut self, ev: &SseEvent, json: Option<&Value>) -> Vec<Bytes> {
        if self.protocol == Protocol::OpenaiChat {
            if let Some(j) = json {
                if let Some(raw) = j.get("usage").filter(|u| !u.is_null()) {
                    let no_choices = j
                        .get("choices")
                        .and_then(Value::as_array)
                        .is_some_and(Vec::is_empty);
                    if self.strip_injected_usage && no_choices {
                        return Vec::new();
                    }
                    if let Some(rendered) = match_usage(raw, self.protocol)
                        .ok()
                        .and_then(|parsed| native_render(self.native, &parsed))
                    {
                        let mut j = j.clone();
                        j["usage"] = rendered;
                        return vec![sse::encode(ev.event.as_deref(), &j)];
                    }
                }
            }
        }
        vec![ev.raw.clone()]
    }
}

enum StreamMode {
    Passthrough(PassthroughStream),
    Convert(Box<StreamConverter>),
}

impl StreamMode {
    fn event(&mut self, ev: &SseEvent, json: Option<&Value>) -> Vec<Bytes> {
        match self {
            StreamMode::Passthrough(p) => p.event(ev, json),
            StreamMode::Convert(c) => c.event(ev, json),
        }
    }

    fn finish(&mut self) -> Vec<Bytes> {
        match self {
            StreamMode::Passthrough(_) => Vec::new(),
            StreamMode::Convert(c) => c.finish(),
        }
    }
}

fn stream_response(ctx: Ctx, response: reqwest::Response) -> Response {
    let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::OK);
    let (headers, mut mode) = match ctx.mode {
        RelayMode::Passthrough => (
            upstream::response_headers(response.headers()),
            StreamMode::Passthrough(PassthroughStream {
                protocol: ctx.inbound,
                strip_injected_usage: ctx.strip_injected_usage,
                native: ctx.native,
            }),
        ),
        RelayMode::Convert => (
            sse_headers(),
            StreamMode::Convert(Box::new(StreamConverter::new(
                ctx.upstream_dialect,
                ctx.client,
                ctx.billing.model.clone(),
                ctx.client_wants_usage,
            ))),
        ),
    };

    let mut byte_stream = response.bytes_stream();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);

    tokio::spawn(async move {
        let mut parser = SseParser::default();
        let mut collector = UsageCollector::new(ctx.upstream_dialect);
        let mut client_gone = false;
        let mut stream_error = None;

        let mut handle = |ev: SseEvent, collector: &mut UsageCollector| -> Vec<Bytes> {
            let json = ev.json();
            if let Some(j) = &json {
                collector.observe(ev.kind(Some(j)), j);
            }
            mode.event(&ev, json.as_ref())
        };

        // Keep reading after the client disconnects so the upstream's usage is still billed.
        while let Some(chunk) = byte_stream.next().await {
            let bytes = match chunk {
                Ok(b) => b,
                Err(e) => {
                    stream_error = Some(format!("upstream stream error: {e}"));
                    break;
                }
            };
            for ev in parser.push(&bytes) {
                for out in handle(ev, &mut collector) {
                    if !client_gone && tx.send(Ok(out)).await.is_err() {
                        client_gone = true;
                    }
                }
            }
        }
        let mut tail = Vec::new();
        if let Some(ev) = parser.finish() {
            tail.extend(handle(ev, &mut collector));
        }
        tail.extend(mode.finish());
        for out in tail {
            if !client_gone && tx.send(Ok(out)).await.is_err() {
                client_gone = true;
            }
        }
        if let Some(e) = &stream_error {
            let _ = tx.send(Err(std::io::Error::other(e.clone()))).await;
        }
        drop(tx);
        ctx.report_health(stream_error.is_none());

        let soft_error = collector.soft_error.take();
        let outcome = match soft_error {
            Some(reason) => Err(BillingFailure {
                reason,
                raw_usage: None,
            }),
            None => usage_from_raw(collector.finish(), ctx.upstream),
        };
        finalize(&ctx, &outcome, stream_error).await;
    });

    let body = Body::from_stream(stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    }));
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

pub async fn list_models(state: &RelayState, api_key: &str) -> RelayResult<Response> {
    let auth = token_svc::authenticate_api_key(&state.pool, api_key).await?;
    let group = token_svc::resolve_group(&auth.token, &auth.user);
    let models = channel::list_available_models(&state.pool, &group).await?;
    let data: Vec<Value> = models
        .into_iter()
        .filter(|m| auth.token.allows_model(m))
        .map(|id| {
            json!({
                "id": id,
                "object": "model",
                "owned_by": "danmo"
            })
        })
        .collect();

    Ok(axum::Json(json!({
        "object": "list",
        "data": data
    }))
    .into_response())
}
