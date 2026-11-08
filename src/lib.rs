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
//! anyone else sees of them unless they choose to show their Gravatar too
//! (see `gravatar`). Posting needs a sign-in and a nickname. The queries
//! compare people by an id derived from their email; the email sits in
//! `people` for the operator's audits, and neither it nor anything derived
//! from it leaves the server. Reading needs no account.
//!
//! A message can carry a photo or video. The page uploads it straight to
//! YourImageShare, and the API keeps only the link, after checking that it
//! points to a file there (see `media_kind`).
//!
//! A thread's link, `/t/{id}`, comes here too, for a page that chat apps can
//! preview (see `page`).

mod auth;
mod geo;
mod gravatar;
mod page;

pub use auth::Verifier;
pub use gravatar::Gravatar;

use auth::Person;

use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{FromRef, FromRequestParts, Path, Query, State},
    http::{
        HeaderMap, StatusCode,
        header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS},
        request::Parts,
    },
    response::{IntoResponse, Redirect, Response},
    routing::{get, post, put},
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
/// Characters, not bytes: an emoji is one.
const BODY_MAX: usize = 280;
const PREVIEW_MAX: usize = 140;
/// Where media lives. A link anywhere else is refused, so nobody can post a
/// picture from their own server and learn who opens the thread.
const MEDIA_HOST: &str = "https://yourimageshare.com/ib/";
/// The file types YourImageShare takes, by the extension on its links.
const IMAGE_TYPES: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "avif", "bmp", "tif", "tiff", "heic", "heif"];
const VIDEO_TYPES: &[&str] = &["mp4", "webm", "mov", "avi", "mkv", "mpeg", "mpg", "wmv", "flv", "3gp"];
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
/// real, a test key in tests. `gravatar` is where pictures come from:
/// [`Gravatar::public`] for real, a stand-in in tests.
pub fn app(pool: PgPool, verifier: Verifier, gravatar: Gravatar) -> Router {
    Router::new()
        .route("/api/threads", get(list_threads).post(create_thread))
        .route("/api/threads/{id}", get(get_thread))
        .route("/api/threads/{id}/messages", post(add_message))
        .route("/api/me", get(me).put(set_nickname))
        .route("/api/me/gravatar", put(set_gravatar))
        .route("/api/avatars/{id}", get(avatar))
        .route("/api/sign-in", get(sign_in))
        .route("/t/{id}", get(thread_page))
        .route("/t/{id}/", get(thread_page))
        .with_state(AppState { pool, verifier: Arc::new(verifier), gravatar, http: page::client() })
}

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    verifier: Arc<Verifier>,
    gravatar: Gravatar,
    /// Fetches the app's page for a thread's link.
    http: reqwest::Client,
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
    /// Not available: not set up on this deployment, such as sign-in before
    /// registration, or Gravatar not answering.
    Unavailable(String),
    /// No such thread now: it expired (and may have been deleted since), or
    /// it never existed. To a visitor following a link they look the same.
    NotFound,
    /// No picture to show: its owner hid it, or Gravatar has none for them.
    NoPicture,
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ApiError::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message),
            ApiError::Forbidden(message) => (StatusCode::FORBIDDEN, message),
            ApiError::Conflict(message) => (StatusCode::CONFLICT, message),
            ApiError::Unavailable(message) => (StatusCode::SERVICE_UNAVAILABLE, message),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "This thread is gone.".to_owned()),
            ApiError::NoPicture => (StatusCode::NOT_FOUND, "There's no picture here.".to_owned()),
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
    /// A link to the author's Gravatar, while they choose to show it.
    avatar: Option<String>,
    /// Empty when the message is only a photo or video.
    body: String,
    media: Option<Media>,
    created_at: DateTime<Utc>,
    /// Posted by the asker. Ids stay on the server, so nobody can post as
    /// someone else just by reading a thread.
    mine: bool,
}

/// A photo or video posted with a message: a file on YourImageShare.
#[derive(Serialize)]
struct Media {
    url: String,
    /// `image` or `video`, so the page knows how to show it.
    kind: &'static str,
}

/// A message as stored, with its author's nickname and Gravatar id as they are now.
#[derive(sqlx::FromRow)]
struct MessageRow {
    id: i64,
    author_id: Uuid,
    author_name: String,
    gravatar: Option<Uuid>,
    body: String,
    media: Option<String>,
    created_at: DateTime<Utc>,
}

async fn get_thread(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Author(viewer): Author,
) -> Result<Json<Thread>, ApiError> {
    Ok(Json(load_thread(&pool, id, viewer.map(|person| person.id)).await?))
}

#[derive(Deserialize)]
struct SignInQuery {
    state: String,
}

/// Starts a sign-in by sending the browser to Aero's consent screen. The
/// page makes up `state` and keeps it, to check Aero's answer against; the
/// client id stays on the server, next to the key it belongs with.
async fn sign_in(State(app): State<AppState>, Query(query): Query<SignInQuery>) -> Result<Redirect, ApiError> {
    let state = query.state;
    let url_safe = state.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !(1..=512).contains(&state.len()) || !url_safe {
        return Err(ApiError::BadRequest("state must be 1 to 512 letters, digits, - or _.".to_owned()));
    }
    let url = app
        .verifier
        .sign_in_url(&state)
        .ok_or_else(|| ApiError::Unavailable("Sign-in isn't set up yet.".to_owned()))?;
    Ok(Redirect::to(&url))
}

/// What the page learns about who is signed in: what others see of them,
/// since it can't open the token. No email, even to its owner.
#[derive(Serialize)]
struct Me {
    /// None until they choose one; posting needs it.
    nickname: Option<String>,
    /// A link to their Gravatar, while they choose to show it.
    avatar: Option<String>,
}

async fn me(State(pool): State<PgPool>, author: Author) -> Result<Json<Me>, ApiError> {
    let person = author.required()?;
    let row: Option<(String, Option<Uuid>)> =
        sqlx::query_as("SELECT nickname, gravatar FROM people WHERE id = $1")
            .bind(person.id)
            .fetch_optional(&pool)
            .await?;
    Ok(Json(match row {
        Some((nickname, gravatar)) => Me { nickname: Some(nickname), avatar: avatar_link(gravatar) },
        None => Me { nickname: None, avatar: None },
    }))
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
    let saved: Result<(Option<Uuid>,), _> = sqlx::query_as(
        "INSERT INTO people (id, email, nickname) VALUES ($1, $2, $3)
         ON CONFLICT (id) DO UPDATE SET nickname = EXCLUDED.nickname, email = EXCLUDED.email
         RETURNING gravatar",
    )
    .bind(person.id)
    .bind(&person.email)
    .bind(&nickname)
    .fetch_one(&pool)
    .await;
    match saved {
        Ok((gravatar,)) => Ok(Json(Me { nickname: Some(nickname), avatar: avatar_link(gravatar) })),
        // Only `people_nickname` can clash: `id` conflicts are handled above.
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Err(ApiError::Conflict("That nickname is taken.".to_owned()))
        }
        Err(e) => Err(e.into()),
    }
}

#[derive(Deserialize)]
struct GravatarChoice {
    show: bool,
}

/// Shows or hides the signed-in person's Gravatar next to their posts.
/// Showing it gives the picture a random id; hiding it forgets the id, so
/// links to it stop working, and showing it again makes a new one.
async fn set_gravatar(
    State(pool): State<PgPool>,
    author: Author,
    Json(choice): Json<GravatarChoice>,
) -> Result<Json<Me>, ApiError> {
    let person = author.required()?;
    let row: Option<(String, Option<Uuid>)> = sqlx::query_as(
        "UPDATE people
            SET gravatar = CASE WHEN $2 THEN COALESCE(gravatar, gen_random_uuid()) END
          WHERE id = $1
      RETURNING nickname, gravatar",
    )
    .bind(person.id)
    .bind(choice.show)
    .fetch_optional(&pool)
    .await?;
    let (nickname, gravatar) = row.ok_or_else(|| ApiError::Forbidden("Choose a nickname first.".to_owned()))?;
    Ok(Json(Me { nickname: Some(nickname), avatar: avatar_link(gravatar) }))
}

/// Someone's Gravatar, while they choose to show it. The API fetches it, so
/// the page never sees the hash of the email that Gravatar files it under.
/// Browsers may keep a copy for an hour, but shared caches keep none, so a
/// hidden picture can't be fetched again.
async fn avatar(State(app): State<AppState>, Path(id): Path<Uuid>) -> Result<Response, ApiError> {
    let row: Option<(String,)> = sqlx::query_as("SELECT email FROM people WHERE gravatar = $1")
        .bind(id)
        .fetch_optional(&app.pool)
        .await?;
    let Some((email,)) = row else {
        return Err(ApiError::NoPicture);
    };
    let picture = app.gravatar.picture(&email).await.map_err(|err| {
        eprintln!("gravatar: {err}");
        ApiError::Unavailable("Gravatar isn't answering. Try again later.".to_owned())
    })?;
    let Some(picture) = picture else {
        return Err(ApiError::NoPicture);
    };
    let headers = [
        (CONTENT_TYPE, picture.content_type),
        (CACHE_CONTROL, "private, max-age=3600"),
        (X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ];
    Ok((headers, picture.bytes).into_response())
}

/// A thread's link as a page: the app, with the thread in the tags chat apps
/// read for a link preview, or word that it is gone (see `page`). A link
/// that names no thread gets the app as it is, which then shows the map. The
/// CDN may keep a page for a minute: the app loads the thread itself, so
/// only a preview can be that far behind.
async fn thread_page(
    State(app): State<AppState>,
    Path(part): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let origin = page::origin(&headers).ok_or_else(|| ApiError::BadRequest("No host to serve the page for.".to_owned()))?;
    let shell = match page::shell(&app.http, &origin).await {
        Ok(shell) => shell,
        Err(err) => {
            eprintln!("page: {err}");
            // Plain words, not JSON: a person opening a link reads this.
            let headers = [(CONTENT_TYPE, "text/plain; charset=utf-8"), (CACHE_CONTROL, "no-store")];
            let words = "Champt's page didn't load. Try again in a moment.";
            return Ok((StatusCode::SERVICE_UNAVAILABLE, headers, words).into_response());
        }
    };
    let html = match page::thread_id(&part) {
        Some(id) => page::with_preview(&shell, &thread_preview(&app.pool, id).await?, &format!("{origin}/t/{id}")),
        None => shell,
    };
    let headers = [(CONTENT_TYPE, "text/html; charset=utf-8"), (CACHE_CONTROL, "public, max-age=0, s-maxage=60")];
    Ok((headers, html).into_response())
}

/// What a link preview shows of a thread: its opening line, how many
/// messages it has, and the photo it opens with, if any; or that it's gone.
async fn thread_preview(pool: &PgPool, id: i64) -> Result<page::Preview, ApiError> {
    let row: Option<(String, i32, Option<String>)> = sqlx::query_as(
        "SELECT t.preview, t.message_count,
                (SELECT m.media FROM messages m WHERE m.thread_id = t.id ORDER BY m.id LIMIT 1)
           FROM threads t
          WHERE t.id = $1 AND t.expires_at > now()",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some((opening, messages, media)) => {
            let photo = media.filter(|link| media_kind(link) == Some("image"));
            page::Preview::thread(opening, messages, photo)
        }
        None => page::Preview::gone(),
    })
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
    let rows: Vec<MessageRow> = sqlx::query_as(
        "SELECT m.id, m.author_id, COALESCE(p.nickname, m.author_name) AS author_name,
                p.gravatar, m.body, m.media, m.created_at
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
        .map(|row| Message {
            id: row.id,
            author_name: row.author_name,
            avatar: avatar_link(row.gravatar),
            body: row.body,
            // Every stored link was checked on the way in.
            media: row.media.map(|url| Media { kind: media_kind(&url).unwrap_or("image"), url }),
            created_at: row.created_at,
            mine: viewer == Some(row.author_id),
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
    /// May be empty when `media` is given.
    body: String,
    /// A link to a file the page uploaded to YourImageShare.
    media: Option<String>,
}

async fn create_thread(
    State(pool): State<PgPool>,
    author: Author,
    Json(new): Json<NewThread>,
) -> Result<(StatusCode, Json<Thread>), ApiError> {
    let person = author.required()?;
    let (lat, lng) = geo::snap(new.lat, new.lng).map_err(|e| ApiError::BadRequest(e.to_owned()))?;
    let media = media_link(new.media)?;
    let body = clean(&new.body, BODY_MAX, "message", media.is_none())?;
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
    .bind(preview(&body, media.as_deref()))
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO messages (thread_id, author_id, author_name, body, media)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(person.id)
    .bind(&name)
    .bind(&body)
    .bind(&media)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(load_thread(&pool, id, Some(person.id)).await?)))
}

#[derive(Deserialize)]
struct NewMessage {
    /// May be empty when `media` is given.
    body: String,
    /// A link to a file the page uploaded to YourImageShare.
    media: Option<String>,
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
    let media = media_link(new.media)?;
    let body = clean(&new.body, BODY_MAX, "message", media.is_none())?;
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
         INSERT INTO messages (thread_id, author_id, author_name, body, media)
         SELECT id, $2, $3, $4, $5 FROM thread
         RETURNING id",
    )
    .bind(id)
    .bind(person.id)
    .bind(&name)
    .bind(&body)
    .bind(&media)
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

/// Where the page fetches a Gravatar shown under `id`.
fn avatar_link(id: Option<Uuid>) -> Option<String> {
    id.map(|id| format!("/api/avatars/{id}"))
}

/// Trims `text` and checks it is no longer than `max` characters, and not
/// empty if `required`.
fn clean(text: &str, max: usize, what: &str, required: bool) -> Result<String, ApiError> {
    let text = text.trim();
    if required && text.is_empty() {
        return Err(ApiError::BadRequest(format!("The {what} is empty.")));
    }
    if text.chars().count() > max {
        return Err(ApiError::BadRequest(format!("The {what} is over {max} characters.")));
    }
    Ok(text.to_owned())
}

/// The media link a post carries, if any: a file on YourImageShare.
fn media_link(link: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(link) = link.map(|link| link.trim().to_owned()).filter(|link| !link.is_empty()) else {
        return Ok(None);
    };
    if media_kind(&link).is_none() {
        return Err(ApiError::BadRequest("Only files uploaded to YourImageShare can be attached.".to_owned()));
    }
    Ok(Some(link))
}

/// `image` or `video` for a link to a file on YourImageShare, by its
/// extension; None for any other link, including the file's page there.
fn media_kind(link: &str) -> Option<&'static str> {
    let (id, extension) = link.strip_prefix(MEDIA_HOST)?.split_once('.')?;
    if !(1..=32).contains(&id.len()) || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let listed = |types: &[&str]| types.iter().any(|t| t.eq_ignore_ascii_case(extension));
    if listed(IMAGE_TYPES) {
        Some("image")
    } else if listed(VIDEO_TYPES) {
        Some("video")
    } else {
        None
    }
}

/// The opening message as the map's thread list shows it, or "Photo" or
/// "Video" when that is all it is.
fn preview(body: &str, media: Option<&str>) -> String {
    let line = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        let video = media.and_then(media_kind) == Some("video");
        return if video { "Video" } else { "Photo" }.to_owned();
    }
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
        assert_eq!(clean("  hi  ", 5, "name", true).ok(), Some("hi".to_owned()));
        assert!(clean("   ", 5, "name", true).is_err());
        assert_eq!(clean("   ", 5, "name", false).ok(), Some(String::new()));
        assert!(clean("abcdef", 5, "name", false).is_err());
        // Characters, not bytes: five emoji fit in five.
        assert!(clean("🙂🙂🙂🙂🙂", 5, "name", true).is_ok());
    }

    #[test]
    fn media_must_be_a_file_on_yourimageshare() {
        assert_eq!(media_kind("https://yourimageshare.com/ib/aB3xY9qRz1.png"), Some("image"));
        assert_eq!(media_kind("https://yourimageshare.com/ib/aB3xY9qRz1.JPG"), Some("image"));
        assert_eq!(media_kind("https://yourimageshare.com/ib/aB3xY9qRz1.mp4"), Some("video"));
        for other in [
            "https://example.com/ib/aB3xY9qRz1.png",
            "http://yourimageshare.com/ib/aB3xY9qRz1.png",
            "https://yourimageshare.com.example.com/ib/aB3xY9qRz1.png",
            // The file's page on YourImageShare, not the file.
            "https://yourimageshare.com/ib/aB3xY9qRz1",
            "https://yourimageshare.com/ib/aB3xY9qRz1.html",
            "https://yourimageshare.com/ib/aB3xY9qRz1.png?size=big",
            "https://yourimageshare.com/ib/../aB3xY9qRz1.png",
            "https://yourimageshare.com/ib/a/b.png",
            "javascript:alert(1)",
        ] {
            assert_eq!(media_kind(other), None, "{other}");
        }
        assert_eq!(media_link(Some("  ".to_owned())).ok(), Some(None));
        assert!(media_link(Some("https://example.com/pixel.gif".to_owned())).is_err());
    }

    #[test]
    fn preview_is_one_line_and_short() {
        assert_eq!(preview("hello\n\nworld", None), "hello world");
        let long = "word ".repeat(100);
        let p = preview(&long, None);
        assert!(p.chars().count() <= PREVIEW_MAX);
        assert!(p.ends_with('…'));
        assert_eq!(preview("short", None), "short");
    }

    #[test]
    fn a_thread_that_opens_with_only_media_says_what_it_is() {
        assert_eq!(preview("", Some("https://yourimageshare.com/ib/aB3xY9qRz1.webp")), "Photo");
        assert_eq!(preview(" \n", Some("https://yourimageshare.com/ib/aB3xY9qRz1.mov")), "Video");
        let words = preview("look", Some("https://yourimageshare.com/ib/aB3xY9qRz1.mov"));
        assert_eq!(words, "look");
    }
}
