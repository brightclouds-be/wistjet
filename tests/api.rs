use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::net::SocketAddr;
use tower::ServiceExt;
use wistjet::{db, server};

struct Reply {
    status: StatusCode,
    cookie: Option<String>,
    content_type: String,
    body: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

fn app_and_state() -> (Router, server::Shared) {
    let conn = db::open_in_memory().unwrap();
    let state = server::AppState::new(
        conn,
        "http://192.168.1.23:8080".into(),
        vec!["wistjet.thuis".into()],
        SETUP_CODE.into(),
    );
    (server::router(state.clone()), state)
}

fn app() -> Router {
    app_and_state().0
}

const SETUP_CODE: &str = "123456";

/// Where a request comes from: the peer address and the `Host` header.
#[derive(Clone, Copy)]
struct From<'a> {
    peer: &'a str,
    host: Option<&'a str>,
}

/// The browser on the computer Wistjet runs on.
const LOCAL: From = From {
    peer: "127.0.0.1:50000",
    host: Some("localhost:8080"),
};

/// A tablet on the home network.
const TABLET: From = From {
    peer: "192.168.1.40:50000",
    host: Some("192.168.1.23:8080"),
};

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> Reply {
    send(app, LOCAL, method, uri, cookie, body).await
}

async fn send(
    app: &Router,
    from: From<'_>,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> Reply {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    if let Some(h) = from.host {
        req = req.header(header::HOST, h);
    }
    let req = match body {
        Some(b) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let peer: SocketAddr = from.peer.parse().unwrap();
    let mut req = req;
    req.extensions_mut().insert(ConnectInfo(peer));
    let res = app.clone().oneshot(req).await.unwrap();
    let header_str = |name| {
        res.headers()
            .get(name)
            .map(|v: &header::HeaderValue| v.to_str().unwrap().to_string())
    };
    Reply {
        status: res.status(),
        cookie: header_str(header::SET_COOKIE),
        content_type: header_str(header::CONTENT_TYPE).unwrap_or_default(),
        body: res.into_body().collect().await.unwrap().to_bytes().to_vec(),
    }
}

fn setup_body() -> Value {
    json!({
        "family_name": "De Smet",
        "timezone": "Europe/Brussels",
        "pin": "2468",
        "child_name": "Lotte",
        "child_avatar": "#e57373",
        "sample_deck": true,
        "setup_code": SETUP_CODE
    })
}

async fn set_up(app: &Router) {
    let r = call(app, "POST", "/api/setup", None, Some(setup_body())).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
}

/// Logs in and returns the `name=value` cookie pair to send back.
async fn log_in(app: &Router) -> String {
    let r = call(
        app,
        "POST",
        "/api/parent/login",
        None,
        Some(json!({ "pin": "2468" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let cookie = r.cookie.unwrap();
    assert!(cookie.contains("HttpOnly"));
    cookie.split(';').next().unwrap().to_string()
}

#[tokio::test]
async fn status_reports_setup_and_lan_url() {
    let app = app();
    let r = call(&app, "GET", "/api/status", None, None).await;
    assert_eq!(
        r.json(),
        json!({ "setup_done": false, "lan_url": "http://192.168.1.23:8080" })
    );
    set_up(&app).await;
    let r = call(&app, "GET", "/api/status", None, None).await;
    assert_eq!(r.json()["setup_done"], json!(true));
}

#[tokio::test]
async fn setup_lists_the_first_child() {
    let app = app();
    set_up(&app).await;
    let r = call(&app, "GET", "/api/children", None, None).await;
    assert_eq!(
        r.json(),
        json!([{ "id": 1, "name": "Lotte", "avatar": "#e57373", "due": 10 }])
    );
}

#[tokio::test]
async fn setup_twice_is_a_conflict() {
    let app = app();
    set_up(&app).await;
    let r = call(&app, "POST", "/api/setup", None, Some(setup_body())).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn invalid_setup_names_the_field() {
    let app = app();
    let mut body = setup_body();
    body["pin"] = json!("12");
    let r = call(&app, "POST", "/api/setup", None, Some(body)).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.json(), json!({ "error": "invalid", "field": "pin" }));
}

#[tokio::test]
async fn parent_routes_need_a_session() {
    let app = app();
    set_up(&app).await;
    for cookie in [None, Some("wistjet_parent=forged"), Some("other=1")] {
        let r = call(&app, "GET", "/api/parent/session", cookie, None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{cookie:?}");
    }
}

#[tokio::test]
async fn right_pin_opens_and_logout_closes_the_parent_area() {
    let app = app();
    set_up(&app).await;
    let cookie = log_in(&app).await;
    let r = call(&app, "GET", "/api/parent/session", Some(&cookie), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "POST", "/api/parent/logout", Some(&cookie), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert!(r.cookie.unwrap().contains("Max-Age=0"));
    let r = call(&app, "GET", "/api/parent/session", Some(&cookie), None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_pin_is_rejected_then_locked_out() {
    let app = app();
    set_up(&app).await;
    let wrong = || Some(json!({ "pin": "0000" }));
    for _ in 0..4 {
        let r = call(&app, "POST", "/api/parent/login", None, wrong()).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert_eq!(r.json(), json!({ "error": "wrong_pin" }));
    }
    let r = call(&app, "POST", "/api/parent/login", None, wrong()).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(r.json(), json!({ "error": "locked", "retry_after": 60 }));
    let right = Some(json!({ "pin": "2468" }));
    let r = call(&app, "POST", "/api/parent/login", None, right).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn login_before_setup_is_a_conflict() {
    let app = app();
    let r = call(
        &app,
        "POST",
        "/api/parent/login",
        None,
        Some(json!({ "pin": "2468" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn lan_qr_is_an_svg() {
    let app = app();
    let r = call(&app, "GET", "/api/lan-qr.svg", None, None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.content_type, "image/svg+xml");
    assert!(String::from_utf8(r.body).unwrap().contains("<svg"));
}

#[tokio::test]
async fn unknown_api_path_is_not_found() {
    let app = app();
    let r = call(&app, "GET", "/api/nope", None, None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

/// Needs the built web app: run `npm --prefix web run build` first.
#[tokio::test]
async fn web_app_is_served_for_non_api_paths() {
    let app = app();
    for path in ["/", "/index.html", "/some/client/route"] {
        let r = call(&app, "GET", path, None, None).await;
        assert_eq!(r.status, StatusCode::OK, "{path}");
        assert!(r.content_type.starts_with("text/html"), "{path}");
    }
}

#[tokio::test]
async fn foreign_host_names_are_refused() {
    let app = app();
    for host in ["evil.example", "evil.example:8080", "192.168.1.23.nip.io"] {
        let from = From {
            peer: "127.0.0.1:50000",
            host: Some(host),
        };
        let r = send(&app, from, "GET", "/api/status", None, None).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{host}");
    }
}

#[tokio::test]
async fn home_network_host_names_are_accepted() {
    let app = app();
    for host in [
        "192.168.1.23:8080",
        "nas:8080",
        "wistjet.local",
        "wistjet.thuis",
    ] {
        let from = From {
            peer: "192.168.1.40:50000",
            host: Some(host),
        };
        let r = send(&app, from, "GET", "/api/status", None, None).await;
        assert_eq!(r.status, StatusCode::OK, "{host}");
    }
}

#[tokio::test]
async fn setup_needs_the_code_from_every_device() {
    // Also from this computer: a reverse proxy on the same machine makes every
    // visitor look local, so a local address proves nothing.
    for from in [LOCAL, TABLET] {
        let app = app();
        let mut body = setup_body();
        body["setup_code"] = json!("");
        let r = send(&app, from, "POST", "/api/setup", None, Some(body)).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        assert_eq!(r.json(), json!({ "error": "setup_code" }));
        let r = send(&app, from, "POST", "/api/setup", None, Some(setup_body())).await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
    }
}

#[tokio::test]
async fn host_header_that_is_not_text_is_refused() {
    let req = Request::builder()
        .uri("/api/status")
        .header(
            header::HOST,
            header::HeaderValue::from_bytes(b"\xe9vil.example").unwrap(),
        )
        .body(Body::empty())
        .unwrap();
    let res = app().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn ten_wrong_setup_codes_close_setup_until_restart() {
    let app = app();
    for _ in 0..10 {
        let mut body = setup_body();
        body["setup_code"] = json!("000000");
        let r = send(&app, TABLET, "POST", "/api/setup", None, Some(body)).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
    }
    for from in [TABLET, LOCAL] {
        let r = send(&app, from, "POST", "/api/setup", None, Some(setup_body())).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        assert_eq!(r.json(), json!({ "error": "setup_code_locked" }));
    }
}

#[tokio::test]
async fn typos_with_the_right_code_do_not_use_up_attempts() {
    let app = app();
    for _ in 0..15 {
        let mut body = setup_body();
        body["pin"] = json!("12"); // invalid field, right code
        let r = send(&app, TABLET, "POST", "/api/setup", None, Some(body)).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
    }
    let r = send(&app, TABLET, "POST", "/api/setup", None, Some(setup_body())).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
}

fn answer_body(id: &str, card_id: i64) -> Value {
    json!({ "id": id, "child_id": 1, "card_id": card_id, "direction": 0, "known": true })
}

#[tokio::test]
async fn child_home_shows_what_is_due() {
    let app = app();
    set_up(&app).await;
    let r = call(&app, "GET", "/api/children/1/home", None, None).await;
    assert_eq!(
        r.json(),
        json!({
            "name": "Lotte",
            "avatar": "#e57373",
            "due": 10,
            "subjects": [{ "id": 1, "name": "Frans", "due": 10 }],
            "answered_today": 0,
            "next_due": null
        })
    );
}

#[tokio::test]
async fn unknown_child_has_no_home() {
    let app = app();
    set_up(&app).await;
    let r = call(&app, "GET", "/api/children/99/home", None, None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn session_lists_due_cards_question_first() {
    let app = app();
    set_up(&app).await;
    let r = call(&app, "GET", "/api/children/1/session", None, None).await;
    let cards = r.json();
    assert_eq!(cards.as_array().unwrap().len(), 10);
    let chat = cards
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["card_id"] == 1)
        .unwrap();
    assert_eq!(
        chat["question"],
        json!({ "text": "le chat", "image": null })
    );
    assert_eq!(chat["answer"], json!({ "text": "de kat", "image": null }));
    assert_eq!(chat["subject"], json!("Frans"));
    assert_eq!(chat["repeat_missed"], json!(true));

    let r = call(
        &app,
        "GET",
        "/api/children/1/session?subject=99",
        None,
        None,
    )
    .await;
    assert_eq!(r.json(), json!([]));
}

#[tokio::test]
async fn answer_counts_once_and_moves_the_home_counts() {
    let app = app();
    set_up(&app).await;
    for _ in 0..2 {
        let r = call(
            &app,
            "POST",
            "/api/answers",
            None,
            Some(answer_body("a1", 1)),
        )
        .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
    }
    let r = call(&app, "GET", "/api/children/1/home", None, None).await;
    assert_eq!(r.json()["answered_today"], json!(1));
    assert_eq!(r.json()["due"], json!(9));
}

#[tokio::test]
async fn answer_for_a_card_the_child_cannot_see_is_not_found() {
    let app = app();
    set_up(&app).await;
    let r = call(
        &app,
        "POST",
        "/api/answers",
        None,
        Some(answer_body("a1", 999)),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn malformed_answers_are_rejected() {
    let app = app();
    set_up(&app).await;
    let mut no_id = answer_body("", 1);
    no_id["id"] = json!("");
    let mut long_id = answer_body("", 1);
    long_id["id"] = json!("x".repeat(65));
    let mut bad_direction = answer_body("a1", 1);
    bad_direction["direction"] = json!(2);
    for body in [no_id, long_id, bad_direction] {
        let r = call(&app, "POST", "/api/answers", None, Some(body)).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn answer_before_setup_is_a_conflict() {
    let app = app();
    let r = call(
        &app,
        "POST",
        "/api/answers",
        None,
        Some(answer_body("a1", 1)),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn card_images_are_served_with_their_type() {
    let (app, state) = app_and_state();
    state
        .db
        .lock()
        .unwrap()
        .execute_batch("INSERT INTO image (id, mime, data) VALUES (1, 'image/png', x'89504E47')")
        .unwrap();
    let r = call(&app, "GET", "/api/images/1", None, None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.content_type, "image/png");
    assert_eq!(r.body, vec![0x89, 0x50, 0x4E, 0x47]);
    let r = call(&app, "GET", "/api/images/2", None, None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

// ---- parent content management (Plan 4a) ----

/// A set-up family with a logged-in parent: (router, cookie).
async fn parent() -> (Router, String) {
    let app = app();
    set_up(&app).await;
    let cookie = log_in(&app).await;
    (app, cookie)
}

#[tokio::test]
async fn content_routes_need_a_parent_session() {
    let app = app();
    set_up(&app).await;
    let routes = [
        ("GET", "/api/parent/children"),
        ("POST", "/api/parent/children"),
        ("PUT", "/api/parent/children/1"),
        ("GET", "/api/parent/subjects"),
        ("GET", "/api/parent/systems"),
        ("GET", "/api/parent/decks"),
        ("PUT", "/api/parent/decks/1"),
        ("GET", "/api/parent/decks/1/cards"),
        ("POST", "/api/parent/decks/1/cards"),
        ("GET", "/api/parent/decks/1/progress"),
        ("POST", "/api/parent/decks/1/reset"),
        ("PUT", "/api/parent/cards/1"),
        ("POST", "/api/parent/images"),
    ];
    for (method, path) in routes {
        let r = call(&app, method, path, None, Some(json!({}))).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[tokio::test]
async fn parent_adds_renames_and_archives_a_child() {
    let (app, c) = parent().await;
    let r = call(
        &app,
        "POST",
        "/api/parent/children",
        Some(&c),
        Some(json!({ "name": "Wout", "avatar": "#4fc3f7" })),
    )
    .await;
    assert_eq!(
        (r.status, r.json()),
        (StatusCode::CREATED, json!({ "id": 2 }))
    );
    let body = json!({ "name": "Wouter", "avatar": "#4fc3f7", "archived": true });
    let r = call(&app, "PUT", "/api/parent/children/2", Some(&c), Some(body)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/children", Some(&c), None).await;
    assert_eq!(
        r.json()[1],
        json!({ "id": 2, "name": "Wouter", "avatar": "#4fc3f7", "archived": true })
    );
    // Archived children are gone from the profile screen.
    let r = call(&app, "GET", "/api/children", None, None).await;
    assert_eq!(r.json().as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_content_names_the_field() {
    let (app, c) = parent().await;
    let r = call(
        &app,
        "POST",
        "/api/parent/children",
        Some(&c),
        Some(json!({ "name": "Wout", "avatar": "blue" })),
    )
    .await;
    assert_eq!(
        (r.status, r.json()),
        (
            StatusCode::BAD_REQUEST,
            json!({ "error": "invalid", "field": "avatar" })
        )
    );
    let r = call(
        &app,
        "PUT",
        "/api/parent/children/99",
        Some(&c),
        Some(json!({ "name": "X", "avatar": "#4fc3f7" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn parent_builds_a_deck_with_cards() {
    let (app, c) = parent().await;
    let r = call(
        &app,
        "POST",
        "/api/parent/subjects",
        Some(&c),
        Some(json!({ "name": "Wiskunde" })),
    )
    .await;
    let subject = r.json()["id"].clone();
    let deck = json!({ "name": "Tafels", "subject_id": subject, "system_id": 1, "direction_mode": "both", "children": [1] });
    let r = call(&app, "POST", "/api/parent/decks", Some(&c), Some(deck)).await;
    assert_eq!(r.status, StatusCode::CREATED);
    let deck_id = r.json()["id"].as_i64().unwrap();

    let cards = json!([
        { "front_text": "3 x 4", "back_text": "12", "notes": "Denk aan 4 + 4 + 4" },
        { "front_text": "6 x 7", "back_text": "42" }
    ]);
    let path = format!("/api/parent/decks/{deck_id}/cards");
    let r = call(&app, "POST", &path, Some(&c), Some(cards)).await;
    assert_eq!(r.status, StatusCode::CREATED);
    let first = r.json()["ids"][0].as_i64().unwrap();

    let off = json!({ "front_text": "3 x 4", "back_text": "12", "active": false });
    let r = call(
        &app,
        "PUT",
        &format!("/api/parent/cards/{first}"),
        Some(&c),
        Some(off),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);

    let r = call(&app, "GET", &path, Some(&c), None).await;
    assert_eq!(r.json()[0]["active"], json!(false));
    let r = call(&app, "GET", "/api/parent/decks", Some(&c), None).await;
    let tafels = r
        .json()
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == deck_id)
        .unwrap()
        .clone();
    assert_eq!(
        (tafels["cards"].clone(), tafels["children"].clone()),
        (json!(1), json!([1]))
    );
    // Both directions of the one active card are now due for Lotte, next to the 10 sample cards.
    let r = call(&app, "GET", "/api/children/1/home", None, None).await;
    assert_eq!(r.json()["due"], json!(12));
}

#[tokio::test]
async fn a_bad_pasted_line_saves_nothing() {
    let (app, c) = parent().await;
    let cards = json!([{ "front_text": "un", "back_text": "een" }, { "front_text": "deux", "back_text": " " }]);
    let r = call(
        &app,
        "POST",
        "/api/parent/decks/1/cards",
        Some(&c),
        Some(cards),
    )
    .await;
    assert_eq!(
        (r.status, r.json()["field"].clone()),
        (StatusCode::BAD_REQUEST, json!("back"))
    );
    let r = call(&app, "GET", "/api/parent/decks/1/cards", Some(&c), None).await;
    assert_eq!(r.json().as_array().unwrap().len(), 10);
}

async fn upload(app: &Router, cookie: &str, bytes: Vec<u8>) -> Reply {
    upload_as(app, cookie, "image/png", bytes).await
}

async fn upload_as(app: &Router, cookie: &str, content_type: &str, bytes: Vec<u8>) -> Reply {
    let req = Request::builder()
        .method("POST")
        .uri("/api/parent/images")
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    Reply {
        status: res.status(),
        cookie: None,
        content_type: String::new(),
        body: res.into_body().collect().await.unwrap().to_bytes().to_vec(),
    }
}

#[tokio::test]
async fn images_upload_and_show_on_a_card() {
    let (app, c) = parent().await;
    let png = b"\x89PNG\r\n\x1a\nnot really a picture".to_vec();
    let r = upload(&app, &c, png.clone()).await;
    assert_eq!(r.status, StatusCode::CREATED);
    let id = r.json()["id"].as_i64().unwrap();
    let card = json!({ "front_text": "", "front_image": id, "back_text": "de kat" });
    let r = call(&app, "PUT", "/api/parent/cards/1", Some(&c), Some(card)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", &format!("/api/images/{id}"), None, None).await;
    assert_eq!((r.content_type.as_str(), r.body), ("image/png", png));

    let r = upload(&app, &c, b"<svg onload=alert(1)>".to_vec()).await;
    assert_eq!(
        r.json(),
        json!({ "error": "invalid", "field": "image_type" })
    );
    let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
    big.resize(5 * 1024 * 1024 + 1, 0);
    let r = upload(&app, &c, big).await;
    assert_eq!(
        r.json(),
        json!({ "error": "invalid", "field": "image_size" })
    );
}

#[tokio::test]
async fn deck_progress_and_reset() {
    let (app, c) = parent().await;
    let r = call(
        &app,
        "POST",
        "/api/answers",
        None,
        Some(answer_body("a1", 1)),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/decks/1/progress", Some(&c), None).await;
    assert_eq!(
        r.json(),
        json!([{ "child_id": 1, "name": "Lotte", "due": 9, "boxes": [9, 1, 0, 0] }])
    );

    let r = call(
        &app,
        "POST",
        "/api/parent/decks/1/reset",
        Some(&c),
        Some(json!({ "child_id": 1 })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/decks/1/progress", Some(&c), None).await;
    assert_eq!(r.json()[0]["boxes"], json!([10, 0, 0, 0]));

    let r = call(
        &app,
        "POST",
        "/api/parent/decks/99/reset",
        Some(&c),
        Some(json!({})),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn uploads_a_plain_form_could_send_are_refused() {
    // A form on another site on the same host (same-site, so the cookie goes along) can
    // only send these types; requiring image/* forces a CORS preflight it can't pass.
    let (app, c) = parent().await;
    let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
    for content_type in [
        "text/plain",
        "multipart/form-data; boundary=x",
        "application/x-www-form-urlencoded",
    ] {
        let r = upload_as(&app, &c, content_type, png.clone()).await;
        assert_eq!(
            r.status,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "{content_type}"
        );
    }
}

// ---- study systems, overview and settings (Plan 4b) ----

fn custom_system() -> Value {
    json!({
        "name": "Thuis",
        "repeat_missed": false,
        "boxes": [
            { "name": "Nieuw", "interval_days": 1, "success": 1, "failure": 0 },
            { "name": "Gekend", "interval_days": 5, "success": 1, "failure": 0 }
        ]
    })
}

#[tokio::test]
async fn system_overview_and_settings_routes_need_a_parent_session() {
    let app = app();
    set_up(&app).await;
    let routes = [
        ("POST", "/api/parent/systems"),
        ("PUT", "/api/parent/systems/1"),
        ("DELETE", "/api/parent/systems/1"),
        ("GET", "/api/parent/overview"),
        ("GET", "/api/parent/settings"),
        ("PUT", "/api/parent/settings"),
        ("POST", "/api/parent/pin"),
    ];
    for (method, path) in routes {
        let r = call(&app, method, path, None, Some(json!({}))).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[tokio::test]
async fn parent_creates_edits_and_deletes_a_study_system() {
    let (app, c) = parent().await;
    let r = call(&app, "GET", "/api/parent/systems", Some(&c), None).await;
    let preset = r.json()[0].clone();
    assert_eq!(
        (
            preset["preset"].clone(),
            preset["is_default"].clone(),
            preset["decks"].clone()
        ),
        (json!(true), json!(true), json!(1))
    );
    assert_eq!(preset["boxes"].as_array().unwrap().len(), 4);

    let r = call(
        &app,
        "POST",
        "/api/parent/systems",
        Some(&c),
        Some(custom_system()),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED);
    let id = r.json()["id"].as_i64().unwrap();
    let path = format!("/api/parent/systems/{id}");
    let mut edited = custom_system();
    edited["boxes"][1]["interval_days"] = json!(10);
    let r = call(&app, "PUT", &path, Some(&c), Some(edited)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);

    let r = call(
        &app,
        "PUT",
        "/api/parent/systems/1",
        Some(&c),
        Some(custom_system()),
    )
    .await;
    assert_eq!(
        (r.status, r.json()),
        (
            StatusCode::CONFLICT,
            json!({ "error": "preset", "count": 0 })
        )
    );
    let r = call(&app, "DELETE", "/api/parent/systems/1", Some(&c), None).await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    let r = call(&app, "DELETE", &path, Some(&c), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/systems", Some(&c), None).await;
    assert_eq!(r.json().as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_system_in_use_says_by_how_many_decks() {
    let (app, c) = parent().await;
    let r = call(
        &app,
        "POST",
        "/api/parent/systems",
        Some(&c),
        Some(custom_system()),
    )
    .await;
    let id = r.json()["id"].as_i64().unwrap();
    let deck = json!({ "name": "Voorbeeld", "subject_id": 1, "system_id": id, "direction_mode": "front_to_back", "children": [1] });
    let r = call(&app, "PUT", "/api/parent/decks/1", Some(&c), Some(deck)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(
        &app,
        "DELETE",
        &format!("/api/parent/systems/{id}"),
        Some(&c),
        None,
    )
    .await;
    assert_eq!(
        (r.status, r.json()),
        (
            StatusCode::CONFLICT,
            json!({ "error": "in_use", "count": 1 })
        )
    );
}

#[tokio::test]
async fn overview_shows_today_per_child() {
    let (app, c) = parent().await;
    call(
        &app,
        "POST",
        "/api/answers",
        None,
        Some(answer_body("a1", 1)),
    )
    .await;
    let r = call(&app, "GET", "/api/parent/overview", Some(&c), None).await;
    assert_eq!(
        r.json(),
        json!([{
            "child_id": 1, "name": "Lotte", "avatar": "#e57373",
            "due": 9, "reviewed": 1, "known": 1, "not_known": 0, "boxes": [9, 1]
        }])
    );
}

#[tokio::test]
async fn settings_change_name_timezone_and_default_system() {
    let (app, c) = parent().await;
    let r = call(
        &app,
        "POST",
        "/api/parent/systems",
        Some(&c),
        Some(custom_system()),
    )
    .await;
    let id = r.json()["id"].clone();
    let body = json!({ "name": "Familie De Smet", "timezone": "Europe/Amsterdam", "default_system_id": id });
    let r = call(
        &app,
        "PUT",
        "/api/parent/settings",
        Some(&c),
        Some(body.clone()),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/settings", Some(&c), None).await;
    assert_eq!(r.json(), body);
    let r = call(&app, "GET", "/api/parent/systems", Some(&c), None).await;
    let custom = r
        .json()
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == id)
        .unwrap()
        .clone();
    assert_eq!(custom["is_default"], json!(true));

    let mut bad = body;
    bad["timezone"] = json!("Mars/Olympus");
    let r = call(&app, "PUT", "/api/parent/settings", Some(&c), Some(bad)).await;
    assert_eq!(r.json(), json!({ "error": "invalid", "field": "timezone" }));
}

#[tokio::test]
async fn changing_the_pin_logs_out_other_devices() {
    let (app, mine) = parent().await;
    let other = log_in(&app).await;
    let wrong = json!({ "current": "1111", "new": "13579" });
    let r = call(&app, "POST", "/api/parent/pin", Some(&mine), Some(wrong)).await;
    assert_eq!(
        r.json(),
        json!({ "error": "invalid", "field": "current_pin" })
    );

    let r = call(
        &app,
        "POST",
        "/api/parent/pin",
        Some(&mine),
        Some(json!({ "current": "2468", "new": "13579" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/session", Some(&mine), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/session", Some(&other), None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = call(
        &app,
        "POST",
        "/api/parent/login",
        None,
        Some(json!({ "pin": "13579" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn wrong_current_pins_count_toward_the_lockout() {
    let (app, c) = parent().await;
    let wrong = || Some(json!({ "current": "1111", "new": "13579" }));
    for _ in 0..4 {
        let r = call(&app, "POST", "/api/parent/pin", Some(&c), wrong()).await;
        assert_eq!(r.json()["field"], json!("current_pin"));
    }
    let r = call(&app, "POST", "/api/parent/pin", Some(&c), wrong()).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    // The right current PIN is refused while locked, and so is a login.
    let right = Some(json!({ "current": "2468", "new": "13579" }));
    let r = call(&app, "POST", "/api/parent/pin", Some(&c), right).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    let r = call(
        &app,
        "POST",
        "/api/parent/login",
        None,
        Some(json!({ "pin": "2468" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
}
