//! The API end to end, against the Postgres named by `TEST_DATABASE_URL`.
//! Without it these tests return early (and say so). Point it at a throwaway
//! database: the tests write rows and leave them there.
//!
//! Sign-ins use a throwaway key standing in for the one Aero shares with
//! Champt, and tokens minted the way Aero mints them.

use std::sync::OnceLock;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use pasetors::claims::Claims;
use pasetors::keys::{Generate, SymmetricKey};
use pasetors::paserk::FormatAsPaserk;
use pasetors::{local, version4::V4};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const CLIENT_ID: &str = "test-champt-client-id";

fn aero_key() -> &'static SymmetricKey<V4> {
    static KEY: OnceLock<SymmetricKey<V4>> = OnceLock::new();
    KEY.get_or_init(|| SymmetricKey::<V4>::generate().unwrap())
}

async fn setup() -> Option<(Router, PgPool)> {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL is not set; skipping");
        return None;
    };
    let pool = champt::connect(&url).await.expect("connect to TEST_DATABASE_URL");
    let mut paserk = String::new();
    aero_key().fmt(&mut paserk).unwrap();
    let verifier = champt::Verifier::new(&paserk, CLIENT_ID).unwrap();
    Some((champt::app(pool.clone(), verifier), pool))
}

/// Claims as Aero writes them: `iat` and `nbf` now, `exp` in an hour.
fn claims(aud: &str, email: &str) -> Claims {
    let mut claims = Claims::new().unwrap();
    claims.audience(aud).unwrap();
    claims.add_additional("state", "s").unwrap();
    claims.add_additional("email", email).unwrap();
    claims
}

/// A token like Aero's: `claims` encrypted with `key`.
fn token(key: &SymmetricKey<V4>, claims: &Claims) -> String {
    local::encrypt(key, claims, None, None).unwrap()
}

/// A valid sign-in for `email`, with no nickname yet.
fn signed_in(email: &str) -> String {
    token(aero_key(), &claims(CLIENT_ID, email))
}

/// A signed-in person with a nickname made from their email
/// (`ada.l@example.com` posts as `ada_l_t`), ready to post.
async fn member(app: &Router, email: &str) -> String {
    let who = signed_in(email);
    let local_part = email.split('@').next().unwrap().to_lowercase();
    let nickname: String = local_part.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let (status, answer) = nickname_as(app, &who, &format!("{nickname}_t")).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    who
}

async fn nickname_as(app: &Router, who: &str, nickname: &str) -> (StatusCode, Value) {
    call(app, "PUT", "/api/me", Some(who), Some(json!({ "nickname": nickname }))).await
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    who: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(token) = who {
        request = request.header("authorization", format!("Bearer {token}"));
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

async fn start(app: &Router, who: &str, lat: f64, lng: f64) -> i64 {
    let body = json!({ "lat": lat, "lng": lng, "body": "hello" });
    let (status, thread) = call(app, "POST", "/api/threads", Some(who), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{thread}");
    thread["id"].as_i64().unwrap()
}

/// Replies from `at`, a `(lat, lng)` position.
async fn reply(app: &Router, id: i64, who: &str, body: &str, at: (f64, f64)) -> (StatusCode, Value) {
    let uri = format!("/api/threads/{id}/messages");
    let message = json!({ "body": body, "lat": at.0, "lng": at.1 });
    call(app, "POST", &uri, Some(who), Some(message)).await
}

const PARIS: (f64, f64) = (48.858, 2.294);

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
    let (a, b) = (member(&app, "a@example.com").await, member(&app, "b@example.com").await);
    let id = start(&app, &a, PARIS.0, PARIS.1).await;
    assert!(seconds_left(&pool, id).await > FRESH);

    // The starter talking to themselves buys no time...
    age(&pool, id).await;
    let (status, thread) = reply(&app, id, &a, "anyone?", PARIS).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(thread["reply_extends"], false);
    assert!(seconds_left(&pool, id).await < 3600.0);

    // ...not even from a second sign-in with the email in other letters...
    let a_again = signed_in("A@Example.COM");
    reply(&app, id, &a_again, "hello?", PARIS).await;
    assert!(seconds_left(&pool, id).await < 3600.0);

    // ...someone else answering does.
    let (status, thread) = reply(&app, id, &b, "hi!", PARIS).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(thread["reply_extends"], false); // b is now the last to post
    assert!(seconds_left(&pool, id).await > FRESH);

    // The same with the roles swapped.
    age(&pool, id).await;
    reply(&app, id, &b, "still here", PARIS).await;
    assert!(seconds_left(&pool, id).await < 3600.0);
    reply(&app, id, &a, "me too", PARIS).await;
    assert!(seconds_left(&pool, id).await > FRESH);

    // Each reader learns whether their reply would count, and which messages are theirs.
    let uri = format!("/api/threads/{id}");
    let (_, as_a) = call(&app, "GET", &uri, Some(&a), None).await;
    let (_, as_b) = call(&app, "GET", &uri, Some(&b), None).await;
    let (_, as_anyone) = call(&app, "GET", &uri, None, None).await;
    assert_eq!(as_a["reply_extends"], false);
    assert_eq!(as_b["reply_extends"], true);
    assert_eq!(as_anyone["reply_extends"], true);
    let mine = |thread: &Value| -> Vec<bool> {
        thread["messages"].as_array().unwrap().iter().map(|m| m["mine"].as_bool().unwrap()).collect()
    };
    assert_eq!(mine(&as_a), [true, true, true, false, false, true]);
    assert_eq!(mine(&as_b), [false, false, false, true, true, false]);
    assert_eq!(mine(&as_anyone), [false; 6]);
}

#[tokio::test]
async fn only_valid_aero_sign_ins_can_post() {
    let Some((app, _)) = setup().await else { return };
    let body = json!({ "lat": 1.0, "lng": 1.0, "body": "hi" });

    let mut expired = claims(CLIENT_ID, "a@example.com");
    expired.issued_at("2020-01-01T00:00:00+00:00").unwrap();
    expired.not_before("2020-01-01T00:00:00+00:00").unwrap();
    expired.expiration("2020-01-01T01:00:00+00:00").unwrap();
    // One character changed in the encrypted part: the MAC no longer matches.
    let mut tampered = signed_in("a@example.com").into_bytes();
    let middle = tampered.len() / 2;
    tampered[middle] = if tampered[middle] == b'A' { b'B' } else { b'A' };
    let tampered = String::from_utf8(tampered).unwrap();

    for (what, refused) in [
        ("another key", token(&SymmetricKey::<V4>::generate().unwrap(), &claims(CLIENT_ID, "a@example.com"))),
        ("a changed token", tampered),
        ("another app's token", token(aero_key(), &claims("someone-else", "a@example.com"))),
        ("an expired token", token(aero_key(), &expired)),
        ("a JWT", "eyJhbGciOiJub25lIn0.eyJlbWFpbCI6ImFAZXhhbXBsZS5jb20ifQ.".to_owned()),
        ("garbage", "not-a-token".to_owned()),
    ] {
        let (status, answer) = call(&app, "POST", "/api/threads", Some(&refused), Some(body.clone())).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{what}");
        assert!(answer["error"].is_string(), "{what}");
    }

    // Signed out, posting asks for a sign-in...
    let (status, _) = call(&app, "POST", "/api/threads", None, Some(body.clone())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // ...which works, and reading needs none.
    let who = member(&app, "a@example.com").await;
    let (status, thread) = call(&app, "POST", "/api/threads", Some(&who), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    let uri = format!("/api/threads/{}", thread["id"]);
    let (status, _) = call(&app, "GET", &uri, None, None).await;
    assert_eq!(status, StatusCode::OK);
    // A read that looks at who is asking refuses a failing token, so the
    // page knows to sign in again. (The pin list never looks, so it can't.)
    let (status, _) = call(&app, "GET", &uri, Some("not-a-token"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn nicknames_are_unique_ignoring_case() {
    let Some((app, pool)) = setup().await else { return };
    // These two start from scratch on every run.
    sqlx::query("DELETE FROM people WHERE email IN ('ada.nick@example.com', 'grace.nick@example.com')")
        .execute(&pool)
        .await
        .unwrap();
    let (ada, grace) = (signed_in("ada.nick@example.com"), signed_in("grace.nick@example.com"));

    // Signed in but nameless: no posting yet.
    let (status, me) = call(&app, "GET", "/api/me", Some(&ada), None).await;
    assert_eq!((status, me), (StatusCode::OK, json!({ "nickname": null })));
    let (status, body) = call(&app, "POST", "/api/threads", Some(&ada), Some(json!({ "lat": 1.0, "lng": 1.0, "body": "hi" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body["error"].as_str().unwrap().contains("nickname"));

    // First come, first served, ignoring case; and the rules hold.
    assert_eq!(nickname_as(&app, &ada, "Countess_Ada").await.0, StatusCode::OK);
    let (status, body) = nickname_as(&app, &grace, "countess_ADA").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    for bad in ["ab", "has space", "émile", "Admin"] {
        assert_eq!(nickname_as(&app, &grace, bad).await.0, StatusCode::BAD_REQUEST, "{bad}");
    }

    // Posts show the nickname as it is now: a rename shows on old posts at
    // once, and frees the old name for someone else.
    let id = start(&app, &ada, PARIS.0, PARIS.1).await;
    assert_eq!(nickname_as(&app, &ada, "Ada_L").await.0, StatusCode::OK);
    let (_, thread) = call(&app, "GET", &format!("/api/threads/{id}"), None, None).await;
    assert_eq!(thread["messages"][0]["author_name"], "Ada_L");
    assert_eq!(nickname_as(&app, &grace, "countess_ADA").await.0, StatusCode::OK);
}

#[tokio::test]
async fn emails_stay_on_the_server_for_audits() {
    let Some((app, pool)) = setup().await else { return };
    let ada = member(&app, "Ada.Audit@Example.com").await;
    let grace = member(&app, "grace.audit@example.com").await;

    // The page learns only the nickname, never an email, even its own.
    let (status, me) = call(&app, "GET", "/api/me", Some(&ada), None).await;
    assert_eq!((status, me.clone()), (StatusCode::OK, json!({ "nickname": "ada_audit_t" })));
    let (status, _) = call(&app, "GET", "/api/me", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let id = start(&app, &ada, PARIS.0, PARIS.1).await;
    let (_, thread) = reply(&app, id, &grace, "hi", PARIS).await;

    // The operator can tell who wrote what.
    let (emails,): (Vec<String>,) = sqlx::query_as(
        "SELECT array_agg(p.email ORDER BY m.id)
           FROM messages m JOIN people p ON p.id = m.author_id
          WHERE m.thread_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(emails, ["ada.audit@example.com", "grace.audit@example.com"]);

    // No answer carries an email.
    let (_, as_ada) = call(&app, "GET", &format!("/api/threads/{id}"), Some(&ada), None).await;
    let (_, list) = call(&app, "GET", "/api/threads?bbox=2,48,3,49", None, None).await;
    for answer in [me, thread, as_ada, list] {
        assert!(!answer.to_string().contains("example.com"), "{answer}");
    }
}

#[tokio::test]
async fn an_expired_thread_is_gone() {
    let Some((app, pool)) = setup().await else { return };
    let (a, b) = (member(&app, "a@example.com").await, member(&app, "b@example.com").await);
    let id = start(&app, &a, -33.857, 151.215).await;
    sqlx::query("UPDATE threads SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    let (status, _) = call(&app, "GET", &format!("/api/threads/{id}"), Some(&a), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = reply(&app, id, &b, "too late", (-33.857, 151.215)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list) = call(&app, "GET", "/api/threads?bbox=151,-34,152,-33", None, None).await;
    assert!(!ids(&list).contains(&id));

    // Starting any thread deletes the dead ones.
    start(&app, &a, 10.0, 10.0).await;
    let (left,): (i64,) = sqlx::query_as("SELECT count(*) FROM threads WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn only_people_within_100_km_can_reply() {
    let Some((app, _)) = setup().await else { return };
    let (a, b) = (member(&app, "a@example.com").await, member(&app, "b@example.com").await);
    let id = start(&app, &a, PARIS.0, PARIS.1).await;

    // About 94 km north: within reach.
    let (status, thread) = reply(&app, id, &b, "close enough", (49.7, 2.294)).await;
    assert_eq!(status, StatusCode::CREATED, "{thread}");
    assert_eq!(thread["reply_radius_km"], 100.0);

    // About 105 km north: refused, with the distance in the message.
    let (status, body) = reply(&app, id, &b, "too far", (49.8, 2.294)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let error = body["error"].as_str().unwrap();
    assert!(error.contains("within 100 km") && error.contains("105 km away"), "{error}");

    // No position at all.
    let uri = format!("/api/threads/{id}/messages");
    let (status, body) = call(&app, "POST", &uri, Some(&b), Some(json!({ "body": "hi" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("location"));

    // Only the reply from within reach was posted, and reading is open to all.
    let (status, thread) = call(&app, "GET", &format!("/api/threads/{id}"), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(thread["messages"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_view_across_the_antimeridian_shows_both_sides() {
    let Some((app, _)) = setup().await else { return };
    let a = member(&app, "a@example.com").await;
    let east = start(&app, &a, -17.0, 179.9).await;
    let west = start(&app, &a, -17.0, -179.9).await;
    let far = start(&app, &a, -17.0, 100.0).await;

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
    let who = member(&app, "a@example.com").await;
    let body = json!({ "lat": 35.658_58, "lng": 139.745_43, "body": "hi" });
    let (status, thread) = call(&app, "POST", "/api/threads", Some(&who), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(thread["lat"].as_f64(), Some(35.659));
    assert_eq!(thread["lng"].as_f64(), Some(139.745));
}

#[tokio::test]
async fn bad_requests_are_refused() {
    let Some((app, _)) = setup().await else { return };
    let who = member(&app, "a@example.com").await;
    for bad in [
        json!({ "lat": 1.0, "lng": 1.0, "body": "   " }),
        json!({ "lat": 1.0, "lng": 1.0, "body": "x".repeat(501) }),
        json!({ "lat": 89.0, "lng": 1.0, "body": "hi" }),
    ] {
        let (status, body) = call(&app, "POST", "/api/threads", Some(&who), Some(bad.clone())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
        assert!(body["error"].is_string());
    }
    let (status, _) = call(&app, "GET", "/api/threads?bbox=nope", None, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
