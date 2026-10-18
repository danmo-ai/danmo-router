use anyhow::Result;
use axum::http::{header, HeaderValue, Method};
use axum::Router;
use danmo_core::{connect, Config};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}

pub async fn load_config(default_port: u16) -> Result<(Arc<Config>, danmo_core::DbPool)> {
    let config = Arc::new(Config::from_env_with_default_port(default_port)?);
    let pool = connect(&config).await?;
    Ok((config, pool))
}

pub fn build_cors(origins: &[String]) -> CorsLayer {
    let methods = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::OPTIONS,
    ];
    let headers = [
        header::AUTHORIZATION,
        header::CONTENT_TYPE,
        header::ACCEPT,
        header::COOKIE,
    ];

    if origins.iter().any(|o| o == "*") {
        return CorsLayer::new()
            .allow_origin(HeaderValue::from_static("*"))
            .allow_methods(methods)
            .allow_headers(headers);
    }

    let list: Vec<HeaderValue> = origins.iter().filter_map(|o| o.parse().ok()).collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(list))
        .allow_methods(methods)
        .allow_headers(headers)
        .allow_credentials(true)
}

pub async fn serve(service: &str, config: &Config, app: Router) -> Result<()> {
    let cors = build_cors(&config.cors_origins);
    let app = app
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!("{service} listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
