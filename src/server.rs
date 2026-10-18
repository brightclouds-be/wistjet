//! HTTP API and embedded web app. Every `/api/parent/*` route except login and logout
//! requires a parent session (spec §8).

use crate::auth::{self, Auth, Login};
use crate::family::{self, Setup, SetupError};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const COOKIE: &str = "wistjet_parent";

// ponytail: one connection behind a mutex; one family's traffic is tiny.
pub struct AppState {
    pub db: Mutex<Connection>,
    pub auth: Mutex<Auth>,
    /// Address tablets on the home network can use, shown during first run.
    pub lan_url: String,
}

pub type Shared = Arc<AppState>;

impl AppState {
    pub fn new(db: Connection, lan_url: String) -> Shared {
        Arc::new(AppState {
            db: Mutex::new(db),
            auth: Mutex::new(Auth::default()),
            lan_url,
        })
    }
}

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
        .with_state(state)
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

async fn status(State(s): State<Shared>) -> Response {
    match family::is_set_up(&s.db.lock().unwrap()) {
        Ok(done) => Json(json!({ "setup_done": done, "lan_url": s.lan_url })).into_response(),
        Err(e) => internal(e),
    }
}

async fn setup(State(s): State<Shared>, Json(body): Json<Setup>) -> Response {
    match family::create(&mut s.db.lock().unwrap(), &body) {
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
