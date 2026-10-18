//! HTTP API and embedded web app. Every `/api/parent/*` route except login and logout
//! requires a parent session (spec §8). Every request must be addressed to a home-network
//! name, which blocks DNS-rebinding attacks from websites a parent visits.

use crate::auth::{self, Auth, Login};
use crate::family::{self, Setup, SetupError};
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const COOKIE: &str = "wistjet_parent";
/// Wrong setup codes from other devices before setup from them is closed until restart.
const MAX_SETUP_CODE_FAILURES: u32 = 10;

// ponytail: one connection behind a mutex; one family's traffic is tiny.
pub struct AppState {
    pub db: Mutex<Connection>,
    pub auth: Mutex<Auth>,
    /// Address tablets on the home network can use, shown during first run.
    pub lan_url: String,
    /// Extra host names to accept besides IPs, `localhost`, single-label and `.local` names.
    allowed_hosts: Vec<String>,
    /// Six digits printed in the console; needed to finish setup from another device.
    pub setup_code: String,
    setup_code_failures: AtomicU32,
}

pub type Shared = Arc<AppState>;

impl AppState {
    pub fn new(db: Connection, lan_url: String, allowed_hosts: Vec<String>) -> Shared {
        Arc::new(AppState {
            db: Mutex::new(db),
            auth: Mutex::new(Auth::default()),
            lan_url,
            allowed_hosts,
            setup_code: format!("{:06}", rand::random_range(0..1_000_000)),
            setup_code_failures: AtomicU32::new(0),
        })
    }
}

#[derive(rust_embed::Embed)]
#[folder = "web/dist/"]
#[allow_missing = true]
struct Assets;

pub fn router(state: Shared) -> Router {
    let parent = Router::new()
        .route("/session", get(|| async { StatusCode::NO_CONTENT }))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_parent,
        ));
    Router::new()
        .route("/api/status", get(status))
        .route("/api/setup", post(setup))
        .route("/api/children", get(children))
        .route("/api/lan-qr.svg", get(lan_qr))
        .route("/api/parent/login", post(login))
        .route("/api/parent/logout", post(logout))
        .nest("/api/parent", parent)
        .fallback(static_file)
        .layer(middleware::from_fn_with_state(state.clone(), check_host))
        .with_state(state)
}

/// True for names a home network uses: IP addresses, `localhost`, single-label names
/// (`nas`), `.local` names, and names the admin listed in `WISTJET_HOSTS`.
pub fn host_allowed(host: &str, extra: &[String]) -> bool {
    let name = match host.strip_prefix('[') {
        Some(v6) => {
            return v6
                .split(']')
                .next()
                .is_some_and(|ip| ip.parse::<std::net::Ipv6Addr>().is_ok());
        }
        None => host
            .split(':')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    };
    !name.is_empty()
        && (name.parse::<std::net::IpAddr>().is_ok()
            || name == "localhost"
            || !name.contains('.')
            || name.ends_with(".local")
            || extra.iter().any(|h| h.eq_ignore_ascii_case(&name)))
}

async fn check_host(State(s): State<Shared>, req: Request, next: Next) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok());
    match host {
        Some(h) if !host_allowed(h, &s.allowed_hosts) => (
            StatusCode::FORBIDDEN,
            format!(
                "Wistjet weigert de naam {h}. Gebruik het IP-adres, of voeg de naam toe aan WISTJET_HOSTS."
            ),
        )
            .into_response(),
        _ => next.run(req).await,
    }
}

fn is_local(peer: SocketAddr) -> bool {
    peer.ip().to_canonical().is_loopback()
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn internal(e: impl std::fmt::Display) -> Response {
    eprintln!("internal error: {e}");
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|c| c.trim().strip_prefix("wistjet_parent="))
}

async fn require_parent(State(s): State<Shared>, req: Request, next: Next) -> Response {
    let ok = session_token(req.headers())
        .is_some_and(|t| s.auth.lock().unwrap().touch(t, Instant::now()));
    if ok {
        next.run(req).await
    } else {
        error(StatusCode::UNAUTHORIZED, "locked")
    }
}

async fn status(State(s): State<Shared>, ConnectInfo(peer): ConnectInfo<SocketAddr>) -> Response {
    match family::is_set_up(&s.db.lock().unwrap()) {
        Ok(done) => Json(json!({
            "setup_done": done,
            "setup_code_required": !done && !is_local(peer),
            "lan_url": s.lan_url,
        }))
        .into_response(),
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
struct SetupRequest {
    #[serde(flatten)]
    setup: Setup,
    #[serde(default)]
    setup_code: String,
}

async fn setup(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<SetupRequest>,
) -> Response {
    // Setup from the computer Wistjet runs on is trusted; anything else needs the
    // code from the console, so nobody else on the network can claim the PIN first.
    if !is_local(peer) {
        if s.setup_code_failures.load(Ordering::SeqCst) >= MAX_SETUP_CODE_FAILURES {
            return error(StatusCode::FORBIDDEN, "setup_code_locked");
        }
        if body.setup_code != s.setup_code {
            s.setup_code_failures.fetch_add(1, Ordering::SeqCst);
            return error(StatusCode::FORBIDDEN, "setup_code");
        }
    }
    match family::create(&mut s.db.lock().unwrap(), &body.setup) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(SetupError::AlreadyDone) => error(StatusCode::CONFLICT, "already_set_up"),
        Err(SetupError::Invalid(field)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid", "field": field })),
        )
            .into_response(),
        Err(SetupError::Db(e)) => internal(e),
    }
}

async fn children(State(s): State<Shared>) -> Response {
    match family::children(&s.db.lock().unwrap()) {
        Ok(list) => Json(list).into_response(),
        Err(e) => internal(e),
    }
}

async fn lan_qr(State(s): State<Shared>) -> Response {
    match qrcode::QrCode::new(&s.lan_url) {
        Ok(code) => {
            let svg = code
                .render::<qrcode::render::svg::Color>()
                .min_dimensions(200, 200)
                .build();
            ([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response()
        }
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
struct PinBody {
    pin: String,
}

async fn login(State(s): State<Shared>, Json(body): Json<PinBody>) -> Response {
    let hash = family::pin_hash(&s.db.lock().unwrap());
    let hash = match hash {
        Ok(Some(h)) => h,
        Ok(None) => return error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => return internal(e),
    };
    let result = s
        .auth
        .lock()
        .unwrap()
        .login(Instant::now(), || auth::verify_pin(&body.pin, &hash));
    match result {
        Login::Ok(token) => (
            StatusCode::NO_CONTENT,
            [(
                header::SET_COOKIE,
                format!("{COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/"),
            )],
        )
            .into_response(),
        Login::WrongPin => error(StatusCode::UNAUTHORIZED, "wrong_pin"),
        Login::Locked { retry_after } => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "locked", "retry_after": retry_after.as_secs().max(1) })),
        )
            .into_response(),
    }
}

async fn logout(State(s): State<Shared>, headers: HeaderMap) -> Response {
    if let Some(t) = session_token(&headers) {
        s.auth.lock().unwrap().logout(t);
    }
    (
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            format!("{COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"),
        )],
    )
        .into_response()
}

/// Serves the embedded web app; unknown non-API paths get `index.html`.
async fn static_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return error(StatusCode::NOT_FOUND, "not_found");
    }
    match Assets::get(path).or_else(|| Assets::get("index.html")) {
        Some(file) => (
            [(header::CONTENT_TYPE, file.metadata.mimetype().to_string())],
            file.data,
        )
            .into_response(),
        None => error(StatusCode::NOT_FOUND, "not_found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_network_names_are_allowed() {
        for host in [
            "localhost",
            "localhost:8080",
            "127.0.0.1:8080",
            "192.168.1.23",
            "[::1]:8080",
            "[fe80::1]",
            "nas",
            "nas:8080",
            "wistjet.local",
            "WISTJET.LOCAL:80",
        ] {
            assert!(host_allowed(host, &[]), "{host}");
        }
    }

    #[test]
    fn other_names_are_refused_unless_listed() {
        for host in [
            "evil.example",
            "evil.example:8080",
            "192.168.1.23.nip.io",
            "wistjet.thuis",
            "",
        ] {
            assert!(!host_allowed(host, &[]), "{host}");
        }
        let listed = ["wistjet.thuis".to_string()];
        assert!(host_allowed("Wistjet.Thuis:443", &listed));
        assert!(!host_allowed("evil.example", &listed));
    }
}
