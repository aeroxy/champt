//! The API end to end, against the Postgres named by `TEST_DATABASE_URL`.
//! Without it these tests return early (and say so). Point it at a throwaway
//! database: the tests write rows and leave them there.

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn setup() -> Option<(Router, PgPool)> {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL is not set; skipping");
        return None;
    };
    let pool = champt::connect(&url).await.expect("connect to TEST_DATABASE_URL");
    Some((champt::app(pool.clone()), pool))
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    author: Option<Uuid>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(author) = author {
        request = request.header(champt::AUTHOR_HEADER, author.to_string());
    }
    let request = match body {
        Some(body) => request
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn start(app: &Router, author: Uuid, lat: f64, lng: f64) -> i64 {
    let body = json!({ "lat": lat, "lng": lng, "name": "A", "body": "hello" });
    let (status, thread) = call(app, "POST", "/api/threads", Some(author), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{thread}");
    thread["id"].as_i64().unwrap()
}

async fn reply(app: &Router, id: i64, author: Uuid, body: &str) -> (StatusCode, Value) {
    let uri = format!("/api/threads/{id}/messages");
    call(app, "POST", &uri, Some(author), Some(json!({ "name": "x", "body": body }))).await
}

async fn seconds_left(pool: &PgPool, id: i64) -> f64 {
    let (secs,): (f64,) =
        sqlx::query_as("SELECT EXTRACT(EPOCH FROM expires_at - now())::float8 FROM threads WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    secs
}

/// Pretends the thread has been quiet for 23 hours.
async fn age(pool: &PgPool, id: i64) {
    sqlx::query("UPDATE threads SET expires_at = now() + interval '1 hour' WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

fn ids(list: &Value) -> Vec<i64> {
    list["threads"].as_array().unwrap().iter().map(|t| t["id"].as_i64().unwrap()).collect()
}

const FRESH: f64 = 23.9 * 3600.0;

#[tokio::test]
async fn only_a_change_of_turn_resets_the_clock() {
    let Some((app, pool)) = setup().await else { return };
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let id = start(&app, a, 48.858, 2.294).await;
    assert!(seconds_left(&pool, id).await > FRESH);

    // The starter talking to themselves buys no time...
    age(&pool, id).await;
    let (status, thread) = reply(&app, id, a, "anyone?").await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(thread["reply_extends"], false);
    assert!(seconds_left(&pool, id).await < 3600.0);

    // ...someone else answering does.
    let (status, thread) = reply(&app, id, b, "hi!").await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(thread["reply_extends"], false); // b is now the last to post
    assert!(seconds_left(&pool, id).await > FRESH);

    // The same with the roles swapped.
    age(&pool, id).await;
    reply(&app, id, b, "still here").await;
    assert!(seconds_left(&pool, id).await < 3600.0);
    reply(&app, id, a, "me too").await;
    assert!(seconds_left(&pool, id).await > FRESH);

    // Each reader learns whether their reply would count, and which messages are theirs.
    let uri = format!("/api/threads/{id}");
    let (_, as_a) = call(&app, "GET", &uri, Some(a), None).await;
    let (_, as_b) = call(&app, "GET", &uri, Some(b), None).await;
    let (_, as_anyone) = call(&app, "GET", &uri, None, None).await;
    assert_eq!(as_a["reply_extends"], false);
    assert_eq!(as_b["reply_extends"], true);
    assert_eq!(as_anyone["reply_extends"], true);
    let mine = |thread: &Value| -> Vec<bool> {
        thread["messages"].as_array().unwrap().iter().map(|m| m["mine"].as_bool().unwrap()).collect()
    };
    assert_eq!(mine(&as_a), [true, true, false, false, true]);
    assert_eq!(mine(&as_b), [false, false, true, true, false]);
    assert_eq!(mine(&as_anyone), [false; 5]);
}

#[tokio::test]
async fn an_expired_thread_is_gone() {
    let Some((app, pool)) = setup().await else { return };
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let id = start(&app, a, -33.857, 151.215).await;
    sqlx::query("UPDATE threads SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    let (status, _) = call(&app, "GET", &format!("/api/threads/{id}"), Some(a), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = reply(&app, id, b, "too late").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list) = call(&app, "GET", "/api/threads?bbox=151,-34,152,-33", None, None).await;
    assert!(!ids(&list).contains(&id));

    // Starting any thread deletes the dead ones.
    start(&app, a, 10.0, 10.0).await;
    let (left,): (i64,) = sqlx::query_as("SELECT count(*) FROM threads WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn a_view_across_the_antimeridian_shows_both_sides() {
    let Some((app, _)) = setup().await else { return };
    let a = Uuid::new_v4();
    let east = start(&app, a, -17.0, 179.9).await;
    let west = start(&app, a, -17.0, -179.9).await;
    let far = start(&app, a, -17.0, 100.0).await;

    // Fiji, the way the map reports a view straddling the antimeridian.
    let (status, list) = call(&app, "GET", "/api/threads?bbox=175,-20,185,-15", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let shown = ids(&list);
    assert!(shown.contains(&east) && shown.contains(&west), "{shown:?}");
    assert!(!shown.contains(&far));

    // A view on one side sees only that side.
    let (_, list) = call(&app, "GET", "/api/threads?bbox=179,-20,179.95,-15", None, None).await;
    let shown = ids(&list);
    assert!(shown.contains(&east) && !shown.contains(&west), "{shown:?}");
}

#[tokio::test]
async fn pins_are_snapped_to_about_100_m() {
    let Some((app, _)) = setup().await else { return };
    let body = json!({ "lat": 35.658_58, "lng": 139.745_43, "name": "A", "body": "hi" });
    let (status, thread) = call(&app, "POST", "/api/threads", Some(Uuid::new_v4()), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(thread["lat"].as_f64(), Some(35.659));
    assert_eq!(thread["lng"].as_f64(), Some(139.745));
}

#[tokio::test]
async fn bad_requests_are_refused() {
    let Some((app, _)) = setup().await else { return };
    let good = json!({ "lat": 1.0, "lng": 1.0, "name": "A", "body": "hi" });

    let (status, _) = call(&app, "POST", "/api/threads", None, Some(good.clone())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "no author");
    let request = Request::builder()
        .method("POST")
        .uri("/api/threads")
        .header(champt::AUTHOR_HEADER, "not-a-uuid")
        .header("content-type", "application/json")
        .body(Body::from(good.to_string()))
        .unwrap();
    assert_eq!(app.clone().oneshot(request).await.unwrap().status(), StatusCode::BAD_REQUEST);

    let author = Some(Uuid::new_v4());
    for bad in [
        json!({ "lat": 1.0, "lng": 1.0, "name": "A", "body": "   " }),
        json!({ "lat": 1.0, "lng": 1.0, "name": "", "body": "hi" }),
        json!({ "lat": 1.0, "lng": 1.0, "name": "A", "body": "x".repeat(501) }),
        json!({ "lat": 89.0, "lng": 1.0, "name": "A", "body": "hi" }),
    ] {
        let (status, body) = call(&app, "POST", "/api/threads", author, Some(bad.clone())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
        assert!(body["error"].is_string());
    }
    let (status, _) = call(&app, "GET", "/api/threads?bbox=nope", None, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
