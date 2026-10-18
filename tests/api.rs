use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
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

fn app() -> Router {
    let conn = db::open_in_memory().unwrap();
    server::router(server::AppState::new(
        conn,
        "http://192.168.1.23:8080".into(),
    ))
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> Reply {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let req = match body {
        Some(b) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
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
        "sample_deck": true
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
        json!([{ "id": 1, "name": "Lotte", "avatar": "#e57373" }])
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
