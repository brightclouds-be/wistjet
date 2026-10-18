//! Parent routes for content management (spec §7.2–7.4). `server::router` mounts these
//! inside the PIN-guarded `/api/parent` router, so every one needs a parent session.

use crate::manage::{self, CardInput, ChildInput, DeckInput, Error, SubjectInput};
use crate::server::{Shared, error, internal, today};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/children", get(list_children).post(create_child))
        .route("/children/{id}", put(update_child))
        .route("/subjects", get(list_subjects).post(create_subject))
        .route("/subjects/{id}", put(update_subject))
        .route("/systems", get(list_systems))
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
    ok(manage::list_systems(&s.db.lock().unwrap()))
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
