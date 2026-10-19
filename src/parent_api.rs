//! Parent routes for content management (spec §7.2–7.4). `server::router` mounts these
//! inside the PIN-guarded `/api/parent` router, so every one needs a parent session.

use crate::auth::{Login, verify_pin};
use crate::family;
use crate::manage::{self, CardInput, ChildInput, DeckInput, Error, Settings, SubjectInput};
use crate::server::{Shared, error, internal, session_token, today};
use crate::systems::{self, SystemInput};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Instant;

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/children", get(list_children).post(create_child))
        .route("/children/{id}", put(update_child))
        .route("/subjects", get(list_subjects).post(create_subject))
        .route("/subjects/{id}", put(update_subject))
        .route("/systems", get(list_systems).post(create_system))
        .route("/systems/{id}", put(update_system).delete(delete_system))
        .route("/overview", get(overview))
        .route("/settings", get(settings).put(update_settings))
        .route("/pin", post(change_pin))
        .route("/decks", get(list_decks).post(create_deck))
        .route("/decks/{id}", put(update_deck))
        .route("/decks/{id}/cards", get(list_cards).post(create_cards))
        .route("/decks/{id}/progress", get(deck_progress))
        .route("/decks/{id}/reset", post(reset))
        .route("/cards/{id}", put(update_card))
        // Room for a 5 MB image; manage::add_image rejects anything larger with a clear error.
        .route(
            "/images",
            post(add_image).layer(DefaultBodyLimit::max(6 * 1024 * 1024)),
        )
}

fn fail(e: Error) -> Response {
    match e {
        Error::Invalid(field) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid", "field": field })),
        )
            .into_response(),
        Error::NotFound => error(StatusCode::NOT_FOUND, "not_found"),
        Error::Refused(code, count) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": code, "count": count })),
        )
            .into_response(),
        Error::Db(e) => internal(e),
    }
}

fn ok<T: Serialize>(r: manage::Result<T>) -> Response {
    r.map_or_else(fail, |v| Json(v).into_response())
}

fn created(r: manage::Result<i64>) -> Response {
    r.map_or_else(fail, |id| {
        (StatusCode::CREATED, Json(json!({ "id": id }))).into_response()
    })
}

fn done(r: manage::Result<()>) -> Response {
    r.map_or_else(fail, |()| StatusCode::NO_CONTENT.into_response())
}

async fn list_children(State(s): State<Shared>) -> Response {
    ok(manage::list_children(&s.db.lock().unwrap()))
}

async fn create_child(State(s): State<Shared>, Json(c): Json<ChildInput>) -> Response {
    created(manage::create_child(&s.db.lock().unwrap(), &c))
}

async fn update_child(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Json(c): Json<ChildInput>,
) -> Response {
    done(manage::update_child(&s.db.lock().unwrap(), id, &c))
}

async fn list_subjects(State(s): State<Shared>) -> Response {
    ok(manage::list_subjects(&s.db.lock().unwrap()))
}

async fn create_subject(State(s): State<Shared>, Json(b): Json<SubjectInput>) -> Response {
    created(manage::create_subject(&s.db.lock().unwrap(), &b))
}

async fn update_subject(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Json(b): Json<SubjectInput>,
) -> Response {
    done(manage::update_subject(&s.db.lock().unwrap(), id, &b))
}

async fn list_systems(State(s): State<Shared>) -> Response {
    ok(systems::list(&s.db.lock().unwrap()))
}

async fn create_system(State(s): State<Shared>, Json(b): Json<SystemInput>) -> Response {
    created(systems::create(&mut s.db.lock().unwrap(), &b))
}

async fn update_system(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Json(b): Json<SystemInput>,
) -> Response {
    done(systems::update(&mut s.db.lock().unwrap(), id, &b))
}

async fn delete_system(State(s): State<Shared>, Path(id): Path<i64>) -> Response {
    done(systems::delete(&s.db.lock().unwrap(), id))
}

async fn overview(State(s): State<Shared>) -> Response {
    let conn = s.db.lock().unwrap();
    match today(&conn) {
        Ok(Some(t)) => ok(manage::overview(&conn, t)),
        Ok(None) => error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => internal(e),
    }
}

async fn settings(State(s): State<Shared>) -> Response {
    ok(manage::settings(&s.db.lock().unwrap()))
}

async fn update_settings(State(s): State<Shared>, Json(b): Json<Settings>) -> Response {
    done(manage::update_settings(&s.db.lock().unwrap(), &b))
}

#[derive(Deserialize)]
struct PinChange {
    current: String,
    new: String,
}

/// Changes the PIN and ends every other parent session, so other devices must use the new one.
/// A wrong current PIN counts toward the login lockout. Lock order auth → db, as in login, so
/// no login can slip in with the old PIN between the change and ending the other sessions.
async fn change_pin(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(b): Json<PinChange>,
) -> Response {
    let mut auth = s.auth.lock().unwrap();
    let conn = s.db.lock().unwrap();
    let hash = match family::pin_hash(&conn) {
        Ok(Some(h)) => h,
        Ok(None) => return error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => return internal(e),
    };
    match auth.attempt(Instant::now(), || verify_pin(&b.current, &hash)) {
        Ok(()) => {}
        Err(Login::Locked { retry_after }) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": "locked", "retry_after": retry_after.as_secs().max(1) })),
            )
                .into_response();
        }
        Err(_) => return fail(Error::Invalid("current_pin")),
    }
    let result = manage::set_pin(&conn, &b.new);
    if result.is_ok()
        && let Some(token) = session_token(&headers)
    {
        auth.end_other_sessions(token);
    }
    done(result)
}

async fn list_decks(State(s): State<Shared>) -> Response {
    ok(manage::list_decks(&s.db.lock().unwrap()))
}

async fn create_deck(State(s): State<Shared>, Json(d): Json<DeckInput>) -> Response {
    created(manage::create_deck(&mut s.db.lock().unwrap(), &d))
}

async fn update_deck(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Json(d): Json<DeckInput>,
) -> Response {
    done(manage::update_deck(&mut s.db.lock().unwrap(), id, &d))
}

async fn list_cards(State(s): State<Shared>, Path(id): Path<i64>) -> Response {
    ok(manage::list_cards(&s.db.lock().unwrap(), id))
}

/// One card from the card form, or many from "Paste a list"; all or nothing.
async fn create_cards(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Json(cards): Json<Vec<CardInput>>,
) -> Response {
    match manage::create_cards(&mut s.db.lock().unwrap(), id, &cards) {
        Ok(ids) => (StatusCode::CREATED, Json(json!({ "ids": ids }))).into_response(),
        Err(e) => fail(e),
    }
}

async fn update_card(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Json(c): Json<CardInput>,
) -> Response {
    done(manage::update_card(&s.db.lock().unwrap(), id, &c))
}

/// Takes the raw image as the body. Only `image/*` is accepted: an HTML form can only send
/// `text/plain`, `multipart/form-data` or `application/x-www-form-urlencoded`, so a page from
/// another site (or another app on this host, which counts as same-site and gets the parent's
/// cookie) can't upload without a CORS preflight, which this server never answers.
async fn add_image(State(s): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let is_image = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.starts_with("image/"));
    if !is_image {
        return error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "image_type");
    }
    created(manage::add_image(&s.db.lock().unwrap(), &body))
}

async fn deck_progress(State(s): State<Shared>, Path(id): Path<i64>) -> Response {
    let conn = s.db.lock().unwrap();
    match today(&conn) {
        Ok(Some(t)) => ok(manage::deck_progress(&conn, id, t)),
        Ok(None) => error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
struct ResetBody {
    /// `None` resets every assigned child.
    child_id: Option<i64>,
}

async fn reset(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<ResetBody>) -> Response {
    let conn = s.db.lock().unwrap();
    match today(&conn) {
        Ok(Some(t)) => done(manage::reset(&conn, id, b.child_id, t).map(|_| ())),
        Ok(None) => error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => internal(e),
    }
}
