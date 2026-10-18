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
    );
    (server::router(state.clone()), state)
}

fn app() -> Router {
    app_and_state().0
}

/// Where a request comes from: the peer address and the `Host` header.
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
        json!({ "setup_done": false, "setup_code_required": false, "lan_url": "http://192.168.1.23:8080" })
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
async fn setup_from_another_device_needs_the_code() {
    let (app, state) = app_and_state();
    let r = send(&app, TABLET, "GET", "/api/status", None, None).await;
    assert_eq!(r.json()["setup_code_required"], json!(true));
    let r = send(&app, LOCAL, "GET", "/api/status", None, None).await;
    assert_eq!(r.json()["setup_code_required"], json!(false));

    let r = send(&app, TABLET, "POST", "/api/setup", None, Some(setup_body())).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.json(), json!({ "error": "setup_code" }));

    let mut body = setup_body();
    body["setup_code"] = json!(state.setup_code.clone());
    let r = send(&app, TABLET, "POST", "/api/setup", None, Some(body)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = send(&app, TABLET, "GET", "/api/status", None, None).await;
    assert_eq!(r.json()["setup_code_required"], json!(false));
}

#[tokio::test]
async fn ten_wrong_setup_codes_close_setup_from_other_devices() {
    let (app, state) = app_and_state();
    let wrong = if state.setup_code == "000000" {
        "111111"
    } else {
        "000000"
    };
    for _ in 0..10 {
        let mut body = setup_body();
        body["setup_code"] = json!(wrong);
        let r = send(&app, TABLET, "POST", "/api/setup", None, Some(body)).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
    }
    let mut body = setup_body();
    body["setup_code"] = json!(state.setup_code.clone());
    let r = send(&app, TABLET, "POST", "/api/setup", None, Some(body)).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.json(), json!({ "error": "setup_code_locked" }));
    // The computer Wistjet runs on can still finish setup.
    let r = send(&app, LOCAL, "POST", "/api/setup", None, Some(setup_body())).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
}
