//! HTTP API and embedded web app. Every `/api/parent/*` route except login and logout
//! requires a parent session (spec §8). Every request must be addressed to a home-network
//! name, which blocks DNS-rebinding attacks from websites a parent visits.

use crate::auth::{self, Auth, Login};
use crate::backup::BackupConfig;
use crate::family::{self, Setup, SetupError};
use crate::store::{self, Answer, AnswerOutcome};
use crate::{engine, study};
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{NaiveDate, Utc};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const COOKIE: &str = "wistjet_parent";
/// Wrong setup codes before setup is closed until restart (a restart prints a new code).
const MAX_SETUP_CODE_FAILURES: u32 = 10;

// ponytail: one connection behind a mutex; one family's traffic is tiny.
pub struct AppState {
    pub db: Mutex<Connection>,
    pub auth: Mutex<Auth>,
    /// Address tablets on the home network can use, shown during first run.
    pub lan_url: String,
    /// Extra host names to accept besides IPs, `localhost`, single-label and `.local` names.
    allowed_hosts: Vec<String>,
    /// Six digits printed in the console and put in the URL the launcher opens;
    /// needed to finish first-run setup, so nobody else on the network can claim the PIN.
    setup_code: String,
    setup_code_failures: AtomicU32,
    /// Automatic backups (spec §9.2); `None` in tests.
    pub backups: Option<BackupConfig>,
}

pub type Shared = Arc<AppState>;

impl AppState {
    pub fn new(
        db: Connection,
        lan_url: String,
        allowed_hosts: Vec<String>,
        setup_code: String,
        backups: Option<BackupConfig>,
    ) -> Shared {
        Arc::new(AppState {
            db: Mutex::new(db),
            auth: Mutex::new(Auth::default()),
            lan_url,
            allowed_hosts,
            setup_code,
            setup_code_failures: AtomicU32::new(0),
            backups,
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
        .merge(crate::parent_api::routes())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_parent,
        ));
    Router::new()
        .route("/api/status", get(status))
        .route("/api/setup", post(setup))
        .route("/api/children", get(children))
        .route("/api/children/{id}/home", get(child_home))
        .route("/api/children/{id}/session", get(child_session))
        .route("/api/answers", post(answer))
        .route("/api/images/{id}", get(image))
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
    // No Host header at all is fine (browsers always send one); a present one must pass.
    let refused = req.headers().get(header::HOST).map(|h| match h.to_str() {
        Ok(h) if host_allowed(h, &s.allowed_hosts) => None,
        Ok(h) => Some(h.to_string()),
        Err(_) => Some("(onleesbaar)".to_string()),
    });
    match refused.flatten() {
        Some(h) => (
            StatusCode::FORBIDDEN,
            format!(
                "Wistjet weigert de naam {h}. Gebruik het IP-adres, of voeg de naam toe aan WISTJET_HOSTS."
            ),
        )
            .into_response(),
        None => next.run(req).await,
    }
}

pub(crate) fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

pub(crate) fn internal(e: impl std::fmt::Display) -> Response {
    eprintln!("internal error: {e}");
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

pub(crate) fn session_token(headers: &HeaderMap) -> Option<&str> {
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

/// Takes the next setup-code attempt, or `None` once the cap is reached. Locked
/// requests don't count, so the counter can never wrap around to zero.
fn take_attempt(counter: &AtomicU32) -> Option<u32> {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
            (n < MAX_SETUP_CODE_FAILURES).then_some(n + 1)
        })
        .ok()
}

#[derive(Deserialize)]
struct SetupRequest {
    #[serde(flatten)]
    setup: Setup,
    #[serde(default)]
    setup_code: String,
}

async fn setup(State(s): State<Shared>, Json(body): Json<SetupRequest>) -> Response {
    // Required from every device: behind a reverse proxy every visitor looks local.
    // Each request takes a numbered attempt *before* the code is compared, so a burst of
    // simultaneous guesses cannot slip past the cap. A right code gives its attempt back;
    // a wrong one keeps it, so at most MAX_SETUP_CODE_FAILURES wrong codes are ever tried.
    let Some(attempt) = take_attempt(&s.setup_code_failures) else {
        return error(StatusCode::FORBIDDEN, "setup_code_locked");
    };
    if body.setup_code != s.setup_code {
        let code = if attempt + 1 >= MAX_SETUP_CODE_FAILURES {
            "setup_code_locked"
        } else {
            "setup_code"
        };
        return error(StatusCode::FORBIDDEN, code);
    }
    s.setup_code_failures.fetch_sub(1, Ordering::SeqCst);
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

/// The family's calendar day right now; `None` before setup.
pub(crate) fn today(conn: &Connection) -> rusqlite::Result<Option<NaiveDate>> {
    Ok(family::timezone(conn)?.map(|tz| engine::local_today(tz, Utc::now())))
}

/// Profile tiles with their due badge (spec §6.1).
async fn children(State(s): State<Shared>) -> Response {
    let conn = s.db.lock().unwrap();
    let tiles = (|| {
        let today = today(&conn)?;
        family::children(&conn)?
            .into_iter()
            .map(|c| {
                let due = match today {
                    Some(t) => store::due_items(&conn, c.id, None, t)?.len(),
                    None => 0,
                };
                Ok(json!({ "id": c.id, "name": c.name, "avatar": c.avatar, "due": due }))
            })
            .collect::<rusqlite::Result<Vec<_>>>()
    })();
    match tiles {
        Ok(list) => Json(list).into_response(),
        Err(e) => internal(e),
    }
}

async fn child_home(State(s): State<Shared>, Path(id): Path<i64>) -> Response {
    let conn = s.db.lock().unwrap();
    let home = today(&conn).and_then(|t| match t {
        Some(t) => study::home(&conn, id, t),
        None => Ok(None),
    });
    match home {
        Ok(Some(h)) => Json(h).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "not_found"),
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
struct SessionQuery {
    subject: Option<i64>,
}

async fn child_session(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Query(q): Query<SessionQuery>,
) -> Response {
    let conn = s.db.lock().unwrap();
    let cards = today(&conn).and_then(|t| match t {
        Some(t) => study::session(&conn, id, q.subject, t, &mut rand::rng()),
        None => Ok(vec![]),
    });
    match cards {
        Ok(cards) => Json(cards).into_response(),
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
struct AnswerBody {
    id: String,
    child_id: i64,
    card_id: i64,
    direction: u8,
    known: bool,
}

/// Saves a scheduling answer (spec §5.3). Repeats and retries are harmless: the server
/// decides what counts, so every outcome but "not visible" is a plain success.
async fn answer(State(s): State<Shared>, Json(b): Json<AnswerBody>) -> Response {
    if !(1..=64).contains(&b.id.len()) || b.direction > 1 {
        return error(StatusCode::BAD_REQUEST, "invalid");
    }
    let mut conn = s.db.lock().unwrap();
    let today = match today(&conn) {
        Ok(Some(t)) => t,
        Ok(None) => return error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => return internal(e),
    };
    let a = Answer {
        id: b.id,
        child_id: b.child_id,
        card_id: b.card_id,
        direction: b.direction,
        known: b.known,
    };
    match store::record_answer(&mut conn, &a, Utc::now(), today) {
        Ok(AnswerOutcome::NotFound) => error(StatusCode::NOT_FOUND, "not_found"),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal(e),
    }
}

/// Image types a card may show; anything else in the database (only possible through a
/// tampered backup) is never served, so it can't run as a page on Wistjet's address.
const IMAGE_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "image/gif"];

async fn image(State(s): State<Shared>, Path(id): Path<i64>) -> Response {
    match study::image(&s.db.lock().unwrap(), id) {
        Ok(Some((mime, data))) if IMAGE_TYPES.contains(&mime.as_str()) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
                // Opened directly, an image still can't run script or load anything.
                (
                    header::CONTENT_SECURITY_POLICY,
                    "default-src 'none'; sandbox".to_string(),
                ),
            ],
            data,
        )
            .into_response(),
        Ok(_) => error(StatusCode::NOT_FOUND, "not_found"),
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
    // Lock order auth → db, as in the PIN change: the hash is read under the auth lock, so a
    // login can't check against a PIN that a concurrent change just replaced.
    let mut auth = s.auth.lock().unwrap();
    let hash = family::pin_hash(&s.db.lock().unwrap());
    let hash = match hash {
        Ok(Some(h)) => h,
        Ok(None) => return error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => return internal(e),
    };
    let result = auth.login(Instant::now(), || auth::verify_pin(&body.pin, &hash));
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

    #[test]
    fn attempts_stop_counting_at_the_cap() {
        let counter = AtomicU32::new(0);
        for n in 0..MAX_SETUP_CODE_FAILURES {
            assert_eq!(take_attempt(&counter), Some(n));
        }
        for _ in 0..3 {
            assert_eq!(take_attempt(&counter), None);
        }
        // Locked requests must not keep counting: a counter that grows can wrap to 0.
        assert_eq!(counter.load(Ordering::SeqCst), MAX_SETUP_CODE_FAILURES);
    }
}
