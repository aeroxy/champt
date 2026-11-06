//! Champt's API: chat threads pinned to places on a map.
//!
//! A thread lives for 24 hours after its last *turn change*. A post resets
//! the clock only when its author differs from whoever posted last, so it
//! takes two people taking turns to keep a thread alive, and nobody can keep
//! one alive alone. Every read filters on `expires_at > now()`, so nothing
//! runs on a schedule; dead threads are deleted whenever a new one starts.
//!
//! There is no login yet. Each browser sends a random id in the
//! `X-Champt-Author` header, and that id is what "a different person" means.
//! Login replaces [`Author`]'s extractor; the queries stay as they are.

mod geo;

use std::time::Duration;

use axum::{
    Json, Router,
    extract::{FromRequestParts, Path, Query, State},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::geo::BBox;

/// Header carrying the poster's id until there is a login.
pub const AUTHOR_HEADER: &str = "x-champt-author";

const NAME_MAX: usize = 32;
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

pub fn app(pool: PgPool) -> Router {
    Router::new()
        .route("/api/threads", get(list_threads).post(create_thread))
        .route("/api/threads/{id}", get(get_thread))
        .route("/api/threads/{id}/messages", post(add_message))
        .with_state(pool)
}

// ---------------------------------------------------------------- identity

/// Who is asking, if they said. Reads may be anonymous; posts may not.
pub struct Author(Option<Uuid>);

impl Author {
    fn required(self) -> Result<Uuid, ApiError> {
        self.0.ok_or_else(|| ApiError::BadRequest(format!("Send your id in {AUTHOR_HEADER}.")))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Author {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, ApiError> {
        let Some(value) = parts.headers.get(AUTHOR_HEADER) else {
            return Ok(Author(None));
        };
        value
            .to_str()
            .ok()
            .and_then(|text| Uuid::parse_str(text).ok())
            .map(|id| Author(Some(id)))
            .ok_or_else(|| ApiError::BadRequest(format!("{AUTHOR_HEADER} must be a UUID.")))
    }
}

// ------------------------------------------------------------------ errors

pub enum ApiError {
    BadRequest(String),
    /// Never existed, or expired: to a visitor the two look the same.
    NotFound,
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
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
    Ok(Json(load_thread(&pool, id, viewer).await?))
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
    let rows: Vec<(i64, Uuid, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, author_id, author_name, body, created_at
           FROM messages
          WHERE thread_id = $1
          ORDER BY id",
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
        messages,
    })
}

#[derive(Deserialize)]
struct NewThread {
    lat: f64,
    lng: f64,
    name: String,
    body: String,
}

async fn create_thread(
    State(pool): State<PgPool>,
    author: Author,
    Json(new): Json<NewThread>,
) -> Result<(StatusCode, Json<Thread>), ApiError> {
    let author = author.required()?;
    let (lat, lng) = geo::snap(new.lat, new.lng).map_err(|e| ApiError::BadRequest(e.to_owned()))?;
    let name = clean(&new.name, NAME_MAX, "name")?;
    let body = clean(&new.body, BODY_MAX, "message")?;

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
    .bind(author)
    .bind(preview(&body))
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO messages (thread_id, author_id, author_name, body)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(author)
    .bind(&name)
    .bind(&body)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(load_thread(&pool, id, Some(author)).await?)))
}

#[derive(Deserialize)]
struct NewMessage {
    name: String,
    body: String,
}

/// The turn rule lives in this one statement: the clock resets only when the
/// poster is not `last_author`. The row lock on `threads` orders concurrent
/// replies, so each sees the author the previous one left behind.
async fn add_message(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    author: Author,
    Json(new): Json<NewMessage>,
) -> Result<(StatusCode, Json<Thread>), ApiError> {
    let author = author.required()?;
    let name = clean(&new.name, NAME_MAX, "name")?;
    let body = clean(&new.body, BODY_MAX, "message")?;

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
    .bind(author)
    .bind(&name)
    .bind(&body)
    .fetch_optional(&pool)
    .await?;
    if posted.is_none() {
        return Err(ApiError::NotFound);
    }

    Ok((StatusCode::CREATED, Json(load_thread(&pool, id, Some(author)).await?)))
}

// ----------------------------------------------------------------- helpers

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
