//! Champt's API: chat threads pinned to places on a map.
//!
//! A thread lives for 24 hours after its last *turn change*. A post resets
//! the clock only when its author differs from whoever posted last, so it
//! takes two people taking turns to keep a thread alive, and nobody can keep
//! one alive alone. Every read filters on `expires_at > now()`, so nothing
//! runs on a schedule; dead threads are deleted whenever a new one starts.
//!
//! Anyone can read any thread, but only people within [`REPLY_RADIUS_KM`] of
//! it can reply. A reply carries the poster's position for that check, and
//! the position is not stored.
//!
//! People sign in with Aero (see `auth`) and choose a nickname, which is all
//! anyone else sees of them. Posting needs both. The queries compare people
//! by an id derived from their email; the email sits in `people` for the
//! operator's audits and never leaves the server. Reading needs neither.

mod auth;
mod geo;

pub use auth::Verifier;

use auth::Person;

use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{FromRef, FromRequestParts, Path, Query, State},
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::geo::BBox;

/// How far from a thread someone may be and still reply to it. Each thread
/// response carries it, so the page never keeps its own copy.
pub const REPLY_RADIUS_KM: f64 = 100.0;

/// Nicknames: English letters, digits and `_`, unique ignoring case.
const NICKNAME_LEN: std::ops::RangeInclusive<usize> = 3..=20;
/// Nicknames nobody may take, so nobody can pass for the operator.
const RESERVED_NICKNAMES: &[&str] =
    &["admin", "administrator", "aero", "champt", "mod", "moderator", "support", "system", "anonymous"];
const BODY_MAX: usize = 500;
const PREVIEW_MAX: usize = 140;
/// The most pins one viewport returns, most recently extended first.
const LIST_LIMIT: i64 = 500;

/// Key for the lock that serializes migrations ("champt" in ASCII).
const MIGRATION_LOCK: i64 = 0x6368_616d_7074;

/// Connects to Postgres and brings the schema up to date.
///
/// Cold starts can migrate at the same moment, and Postgres's
/// `CREATE TABLE IF NOT EXISTS` is not safe under that race. sqlx's own guard
/// is a session advisory lock, which a transaction-mode pooler (Neon's pooled
/// endpoint) breaks: the unlock can reach another server connection, and the
/// next cold start waits forever. So the whole run happens in one
/// transaction holding a transaction-scoped lock, which ends with the
/// transaction on any connection. Migrations nest as savepoints, so none of
/// them may be a `-- no-transaction` migration.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await?;
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(MIGRATION_LOCK)
        .execute(&mut *tx)
        .await?;
    let mut migrator = sqlx::migrate!();
    migrator.set_locking(false);
    migrator.run(&mut *tx).await?;
    tx.commit().await?;
    Ok(pool)
}

/// `verifier` decides who a sign-in token stands for: [`Verifier::aero`] for
/// real, a test key in tests.
pub fn app(pool: PgPool, verifier: Verifier) -> Router {
    Router::new()
        .route("/api/threads", get(list_threads).post(create_thread))
        .route("/api/threads/{id}", get(get_thread))
        .route("/api/threads/{id}/messages", post(add_message))
        .route("/api/me", get(me).put(set_nickname))
        .with_state(AppState { pool, verifier: Arc::new(verifier) })
}

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    verifier: Arc<Verifier>,
}

impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> PgPool {
        state.pool.clone()
    }
}

// ---------------------------------------------------------------- identity

/// Who is asking: the person a valid sign-in token names, or nobody. Reads
/// may be anonymous; posts may not.
struct Author(Option<Person>);

impl Author {
    fn required(self) -> Result<Person, ApiError> {
        self.0.ok_or_else(|| ApiError::Unauthorized("Sign in with Aero first.".to_owned()))
    }
}

impl FromRequestParts<AppState> for Author {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let Some(value) = parts.headers.get(AUTHORIZATION) else {
            return Ok(Author(None));
        };
        // A token that fails is a 401 even on a read, not "nobody": the page
        // then knows to forget it and sign in again.
        value
            .to_str()
            .ok()
            .and_then(|text| text.strip_prefix("Bearer "))
            .and_then(|token| state.verifier.person(token))
            .map(|person| Author(Some(person)))
            .ok_or_else(|| ApiError::Unauthorized("Your sign-in has expired. Sign in again.".to_owned()))
    }
}

// ------------------------------------------------------------------ errors

pub enum ApiError {
    BadRequest(String),
    /// No sign-in where one is needed, or one that failed verification.
    Unauthorized(String),
    /// Understood, but not allowed: too far away, or no nickname yet.
    Forbidden(String),
    /// Clashes with what is there, such as a nickname someone has.
    Conflict(String),
    /// Never existed, or expired: to a visitor the two look the same.
    NotFound,
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ApiError::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message),
            ApiError::Forbidden(message) => (StatusCode::FORBIDDEN, message),
            ApiError::Conflict(message) => (StatusCode::CONFLICT, message),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "This thread has expired.".to_owned()),
            ApiError::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong. Try again.".to_owned())
            }
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        // Logged here, not sent: the caller only learns that it failed.
        eprintln!("database error: {err}");
        ApiError::Internal
    }
}

// ---------------------------------------------------------------- handlers

#[derive(Deserialize)]
struct ListQuery {
    bbox: String,
}

#[derive(Serialize)]
struct ThreadList {
    threads: Vec<Pin>,
}

#[derive(Serialize, sqlx::FromRow)]
struct Pin {
    id: i64,
    lat: f64,
    lng: f64,
    preview: String,
    messages: i32,
    expires_at: DateTime<Utc>,
}

async fn list_threads(
    State(pool): State<PgPool>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ThreadList>, ApiError> {
    let bbox = BBox::parse(&query.bbox).map_err(|e| ApiError::BadRequest(e.to_owned()))?;
    let threads = sqlx::query_as::<_, Pin>(
        "SELECT id, lat, lng, preview, message_count AS messages, expires_at
           FROM threads
          WHERE expires_at > now()
            AND lat BETWEEN $2 AND $4
            AND CASE WHEN $5 THEN lng >= $1 OR lng <= $3
                     ELSE lng BETWEEN $1 AND $3 END
          ORDER BY expires_at DESC
          LIMIT $6",
    )
    .bind(bbox.west)
    .bind(bbox.south)
    .bind(bbox.east)
    .bind(bbox.north)
    .bind(bbox.wraps)
    .bind(LIST_LIMIT)
    .fetch_all(&pool)
    .await?;
    Ok(Json(ThreadList { threads }))
}

#[derive(Serialize)]
struct Thread {
    id: i64,
    lat: f64,
    lng: f64,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    /// Whether a post from the asker would reset the clock: true unless they
    /// posted last.
    reply_extends: bool,
    /// Replies must come from within this distance of the thread.
    reply_radius_km: f64,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    id: i64,
    author_name: String,
    body: String,
    created_at: DateTime<Utc>,
    /// Posted by the asker. Ids stay on the server, so nobody can post as
    /// someone else just by reading a thread.
    mine: bool,
}

async fn get_thread(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Author(viewer): Author,
) -> Result<Json<Thread>, ApiError> {
    Ok(Json(load_thread(&pool, id, viewer.map(|person| person.id)).await?))
}

/// What the page learns about who is signed in: only their nickname, since
/// it can't open the token. No email, even to its owner.
#[derive(Serialize)]
struct Me {
    /// None until they choose one; posting needs it.
    nickname: Option<String>,
}

async fn me(State(pool): State<PgPool>, author: Author) -> Result<Json<Me>, ApiError> {
    let person = author.required()?;
    let row: Option<(String,)> = sqlx::query_as("SELECT nickname FROM people WHERE id = $1")
        .bind(person.id)
        .fetch_optional(&pool)
        .await?;
    Ok(Json(Me { nickname: row.map(|(nickname,)| nickname) }))
}

#[derive(Deserialize)]
struct NewNickname {
    nickname: String,
}

/// Chooses or changes a nickname. The old one is free for others at once.
async fn set_nickname(
    State(pool): State<PgPool>,
    author: Author,
    Json(new): Json<NewNickname>,
) -> Result<Json<Me>, ApiError> {
    let person = author.required()?;
    let nickname = valid_nickname(&new.nickname)?;
    let saved = sqlx::query(
        "INSERT INTO people (id, email, nickname) VALUES ($1, $2, $3)
         ON CONFLICT (id) DO UPDATE SET nickname = EXCLUDED.nickname, email = EXCLUDED.email",
    )
    .bind(person.id)
    .bind(&person.email)
    .bind(&nickname)
    .execute(&pool)
    .await;
    match saved {
        Ok(_) => Ok(Json(Me { nickname: Some(nickname) })),
        // Only `people_nickname` can clash: `id` conflicts are handled above.
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Err(ApiError::Conflict("That nickname is taken.".to_owned()))
        }
        Err(e) => Err(e.into()),
    }
}

/// The nickname someone posts under, or why they can't post yet.
async fn nickname_of(pool: &PgPool, person: &Person) -> Result<String, ApiError> {
    let row: Option<(String,)> = sqlx::query_as("SELECT nickname FROM people WHERE id = $1")
        .bind(person.id)
        .fetch_optional(pool)
        .await?;
    row.map(|(nickname,)| nickname)
        .ok_or_else(|| ApiError::Forbidden("Choose a nickname before posting.".to_owned()))
}

async fn load_thread(pool: &PgPool, id: i64, viewer: Option<Uuid>) -> Result<Thread, ApiError> {
    let row: Option<(f64, f64, DateTime<Utc>, DateTime<Utc>, Uuid)> = sqlx::query_as(
        "SELECT lat, lng, created_at, expires_at, last_author
           FROM threads
          WHERE id = $1 AND expires_at > now()",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some((lat, lng, created_at, expires_at, last_author)) = row else {
        return Err(ApiError::NotFound);
    };
    // A message shows its author's nickname as it is now, so a name always
    // means one person. `author_name` is the name at the time of posting,
    // for messages whose author has no row in `people`.
    let rows: Vec<(i64, Uuid, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT m.id, m.author_id, COALESCE(p.nickname, m.author_name), m.body, m.created_at
           FROM messages m
           LEFT JOIN people p ON p.id = m.author_id
          WHERE m.thread_id = $1
          ORDER BY m.id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let messages = rows
        .into_iter()
        .map(|(id, author, author_name, body, created_at)| Message {
            id,
            author_name,
            body,
            created_at,
            mine: viewer == Some(author),
        })
        .collect();
    Ok(Thread {
        id,
        lat,
        lng,
        created_at,
        expires_at,
        reply_extends: viewer != Some(last_author),
        reply_radius_km: REPLY_RADIUS_KM,
        messages,
    })
}

#[derive(Deserialize)]
struct NewThread {
    lat: f64,
    lng: f64,
    body: String,
}

async fn create_thread(
    State(pool): State<PgPool>,
    author: Author,
    Json(new): Json<NewThread>,
) -> Result<(StatusCode, Json<Thread>), ApiError> {
    let person = author.required()?;
    let (lat, lng) = geo::snap(new.lat, new.lng).map_err(|e| ApiError::BadRequest(e.to_owned()))?;
    let body = clean(&new.body, BODY_MAX, "message")?;
    let name = nickname_of(&pool, &person).await?;

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM threads WHERE expires_at <= now()")
        .execute(&mut *tx)
        .await?;
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO threads (lat, lng, expires_at, last_author, preview, message_count)
         VALUES ($1, $2, now() + interval '24 hours', $3, $4, 1)
         RETURNING id",
    )
    .bind(lat)
    .bind(lng)
    .bind(person.id)
    .bind(preview(&body))
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO messages (thread_id, author_id, author_name, body)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(person.id)
    .bind(&name)
    .bind(&body)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(load_thread(&pool, id, Some(person.id)).await?)))
}

#[derive(Deserialize)]
struct NewMessage {
    body: String,
    /// Where the poster is, for the reply radius. Optional here so that a
    /// missing position gets a clear answer instead of a parse error.
    lat: Option<f64>,
    lng: Option<f64>,
}

/// The turn rule lives in the one statement below: the clock resets only
/// when the poster is not `last_author`. The row lock on `threads` orders
/// concurrent replies, so each sees the author the previous one left behind.
async fn add_message(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    author: Author,
    Json(new): Json<NewMessage>,
) -> Result<(StatusCode, Json<Thread>), ApiError> {
    let person = author.required()?;
    let (Some(lat), Some(lng)) = (new.lat, new.lng) else {
        return Err(ApiError::BadRequest("Share your location to reply.".to_owned()));
    };
    let here = geo::snap(lat, lng).map_err(|e| ApiError::BadRequest(e.to_owned()))?;
    let body = clean(&new.body, BODY_MAX, "message")?;
    let name = nickname_of(&pool, &person).await?;

    let pin: Option<(f64, f64)> =
        sqlx::query_as("SELECT lat, lng FROM threads WHERE id = $1 AND expires_at > now()")
            .bind(id)
            .fetch_optional(&pool)
            .await?;
    let Some(pin) = pin else {
        return Err(ApiError::NotFound);
    };
    let away = geo::distance_km(here, pin);
    if away > REPLY_RADIUS_KM {
        return Err(ApiError::Forbidden(format!(
            "You can reply to threads within {REPLY_RADIUS_KM:.0} km of you. This one is {away:.0} km away."
        )));
    }

    // If the thread expires between that check and this statement, the
    // statement matches nothing and the reply is a 404 like any other.
    let posted: Option<(i64,)> = sqlx::query_as(
        "WITH thread AS (
             UPDATE threads
                SET expires_at = CASE WHEN last_author <> $2
                                      THEN now() + interval '24 hours'
                                      ELSE expires_at END,
                    last_author = $2,
                    message_count = message_count + 1
              WHERE id = $1 AND expires_at > now()
          RETURNING id
         )
         INSERT INTO messages (thread_id, author_id, author_name, body)
         SELECT id, $2, $3, $4 FROM thread
         RETURNING id",
    )
    .bind(id)
    .bind(person.id)
    .bind(&name)
    .bind(&body)
    .fetch_optional(&pool)
    .await?;
    if posted.is_none() {
        return Err(ApiError::NotFound);
    }

    Ok((StatusCode::CREATED, Json(load_thread(&pool, id, Some(person.id)).await?)))
}

// ----------------------------------------------------------------- helpers

/// Checks a nickname against the rules, keeping the case it was typed in.
fn valid_nickname(text: &str) -> Result<String, ApiError> {
    let nickname = text.trim();
    if !NICKNAME_LEN.contains(&nickname.len()) {
        return Err(ApiError::BadRequest(format!(
            "A nickname is {} to {} characters.",
            NICKNAME_LEN.start(),
            NICKNAME_LEN.end()
        )));
    }
    if !nickname.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(ApiError::BadRequest("A nickname uses only English letters, numbers and _.".to_owned()));
    }
    if RESERVED_NICKNAMES.contains(&nickname.to_ascii_lowercase().as_str()) {
        return Err(ApiError::BadRequest("That nickname is reserved.".to_owned()));
    }
    Ok(nickname.to_owned())
}

/// Trims `text` and checks it is neither empty nor longer than `max` characters.
fn clean(text: &str, max: usize, what: &str) -> Result<String, ApiError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(ApiError::BadRequest(format!("The {what} is empty.")));
    }
    if text.chars().count() > max {
        return Err(ApiError::BadRequest(format!("The {what} is over {max} characters.")));
    }
    Ok(text.to_owned())
}

/// The opening message as the map's thread list shows it.
fn preview(body: &str) -> String {
    let line = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= PREVIEW_MAX {
        return line;
    }
    let cut: String = line.chars().take(PREVIEW_MAX - 1).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nicknames_are_english_letters_digits_and_underscores() {
        assert_eq!(valid_nickname("  Ada_L0velace ").ok(), Some("Ada_L0velace".to_owned()));
        assert!(valid_nickname("abc").is_ok());
        assert!(valid_nickname("a".repeat(20).as_str()).is_ok());
        for bad in ["ab", &"a".repeat(21), "has space", "dash-ed", "émile", "名前名前", "Admin", "CHAMPT"] {
            assert!(valid_nickname(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn clean_trims_and_bounds() {
        assert_eq!(clean("  hi  ", 5, "name").ok(), Some("hi".to_owned()));
        assert!(clean("   ", 5, "name").is_err());
        assert!(clean("abcdef", 5, "name").is_err());
        // Characters, not bytes: five emoji fit in five.
        assert!(clean("🙂🙂🙂🙂🙂", 5, "name").is_ok());
    }

    #[test]
    fn preview_is_one_line_and_short() {
        assert_eq!(preview("hello\n\nworld"), "hello world");
        let long = "word ".repeat(100);
        let p = preview(&long);
        assert!(p.chars().count() <= PREVIEW_MAX);
        assert!(p.ends_with('…'));
        assert_eq!(preview("short"), "short");
    }
}
