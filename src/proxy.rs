use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{Method, Request, Response, StatusCode};
use axum::response::IntoResponse;
use axum::Router;
use reqwest::Client;
use serde::Deserialize;
use tracing::{error, info};

const REQ_WHITELIST: &[&str] = &[
    "range",
    "accept",
    "accept-language",
    "user-agent",
    "cookie",
    "authorization",
    "referer",
];

const RESP_WHITELIST: &[&str] = &[
    "content-type",
    "content-length",
    "content-range",
    "accept-ranges",
    "etag",
    "last-modified",
    "content-disposition",
];

#[derive(Deserialize)]
struct ProxyParams {
    url: String,
}

pub struct ProxyState {
    client: Client,
}

pub struct Proxy {
    base_url: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Proxy {
    pub fn start() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let (port_tx, port_rx) = std::sync::mpsc::channel::<u16>();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let handle = std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = port_tx.send(0);
                    error!("proxy runtime failed: {e}");
                    return;
                }
            };

            rt.block_on(async {
                let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = port_tx.send(0);
                        error!("proxy bind failed: {e}");
                        return;
                    }
                };

                let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
                let _ = port_tx.send(port);
                if port == 0 {
                    return;
                }

                let state = Arc::new(ProxyState {
                    client: Client::new(),
                });

                let app = Router::new()
                    .route("/proxy", axum::routing::get(proxy_handler).head(proxy_handler))
                    .with_state(state);

                let shutdown = async move {
                    let _ = shutdown_rx.await;
                };

                if let Err(e) = axum::serve(listener, app)
                    .with_graceful_shutdown(shutdown)
                    .await
                {
                    error!("proxy server error: {e}");
                }
            });
        });

        let port = port_rx.recv()?;
        if port == 0 {
            return Err("proxy failed to start".into());
        }

        let base_url = format!("http://127.0.0.1:{port}/proxy");
        info!("proxy listening on {base_url}");

        Ok(Self {
            base_url,
            shutdown: Some(shutdown_tx),
            handle: Some(handle),
        })
    }

    pub fn local_url_for(&self, remote_url: &str) -> String {
        let encoded = urlencoding::encode(remote_url);
        format!("{}?url={}", self.base_url, encoded)
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

async fn proxy_handler(
    State(state): State<Arc<ProxyState>>,
    Query(params): Query<ProxyParams>,
    req: Request<Body>,
) -> Response<Body> {
    let decoded = match urlencoding::decode(&params.url) {
        Ok(d) => d.into_owned(),
        Err(e) => {
            error!("failed to decode url {}: {e}", params.url);
            return (StatusCode::BAD_REQUEST, "bad url").into_response();
        }
    };

    let remote_url = match reqwest::Url::parse(&decoded) {
        Ok(u) => u,
        Err(e) => {
            error!("failed to parse url {decoded}: {e}");
            return (StatusCode::BAD_REQUEST, "bad url").into_response();
        }
    };

    if *req.method() != Method::GET && *req.method() != Method::HEAD {
        return (StatusCode::METHOD_NOT_ALLOWED, "only GET/HEAD allowed").into_response();
    }

    info!("proxy {} -> {}", req.method(), remote_url);

    let mut rb = state.client.request(req.method().clone(), remote_url.clone());
    let mut has_user_agent = false;
    for (name, value) in req.headers() {
        if REQ_WHITELIST.contains(&name.as_str().to_lowercase().as_str()) {
            rb = rb.header(name, value);
            if name.as_str().to_lowercase().as_str() == "user-agent" {
                has_user_agent = true;
            }
        }
    }
    if !has_user_agent {
        rb = rb.header("user-agent", "min-mpv/0.1.0");
    }

    let upstream = match rb.send().await {
        Ok(r) => r,
        Err(e) => {
            error!("proxy request failed for {remote_url}: {e}");
            return (StatusCode::BAD_GATEWAY, "upstream error").into_response();
        }
    };

    let status = upstream.status();
    info!("proxy response {} for {}", status, remote_url);

    let mut resp = Response::builder().status(status);
    for (name, value) in upstream.headers() {
        if RESP_WHITELIST.contains(&name.as_str().to_lowercase().as_str()) {
            resp = resp.header(name, value);
        }
    }

    let body = if req.method() == Method::HEAD {
        Body::empty()
    } else {
        Body::from_stream(upstream.bytes_stream())
    };

    match resp.body(body) {
        Ok(r) => r,
        Err(e) => {
            error!("proxy response build error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "proxy error").into_response()
        }
    }
}

