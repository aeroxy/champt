# Champt

Chat threads pinned to places on a map. You start a thread where you are. Anyone can read it from anywhere, and anyone within 100 km of it can reply. A thread disappears after 24 hours unless someone *else* replies: a reply resets the clock only when its author is not the one who posted last, so it takes two people taking turns to keep a thread alive.

The 2013 version (Node and Socket.IO, live dots on a map) is on the `archive` branch.

## How it behaves

| Rule | Where it lives |
|---|---|
| A thread starts only where you are, snapped to a 0.001° grid (about 100 m) | `snap` in `src/geo.rs`; the browser rounds before sending, too |
| A thread lives 24 hours past its last turn change | one `UPDATE` in `add_message`, `src/lib.rs`; every read filters on `expires_at > now()` |
| The map pans worldwide, the antimeridian included, and any thread can be read | `list_threads`, `BBox` in `src/geo.rs` |
| Replies come only from within 100 km of a thread. The poster's position travels with the reply for the check and is not stored | `REPLY_RADIUS_KM` and `add_message` in `src/lib.rs`; the reply box in `frontend/src/Panel.tsx` |
| No login yet: each browser keeps a random id, sent as `X-Champt-Author` | `Author` in `src/lib.rs`, `frontend/src/api.ts` |
| No live updates: Refresh reloads the pins and the open thread | `frontend/src/App.tsx` |

Expired threads are deleted whenever someone starts a new one, so nothing runs on a schedule.

## What runs where

| Piece | Choice |
|---|---|
| API | Rust: axum 0.8 and sqlx with raw SQL, as one Vercel Function (`api/server.rs`) through `vercel_runtime`'s axum layer. Vercel's Rust runtime is in beta. |
| Database | Postgres. Neon works through either its pooled or its direct connection string. Migrations in `migrations/` run when a function instance starts. |
| Front end | React 19, Vite and Tailwind 4, built with bun and served from Vercel's CDN |
| Map | MapLibre GL with OpenFreeMap's `positron` tiles: no key, no account |

## Local development

```sh
cp .env.example .env          # set DATABASE_URL
cargo run --bin dev           # the API on http://127.0.0.1:8787
cd frontend && bun install && bun run dev   # http://localhost:5176, proxying /api
```

Browsers only share location with secure pages. `localhost` counts; a LAN address over plain http does not.

## Tests

```sh
cargo test                                         # unit tests
TEST_DATABASE_URL=postgres://... cargo test        # and the API against Postgres
cd frontend && bun run typecheck
```

The API tests write rows and leave them, so give them a throwaway database. Without `TEST_DATABASE_URL` they return early.

## Deploying on Vercel

1. Import the GitHub repository as a Vercel project. `vercel.json` does the rest: bun builds `frontend/` into static files, Vercel builds `api/server.rs` as a function, and `/api/*` is rewritten to it.
2. Give it a Postgres database as `DATABASE_URL`, in AWS Singapore (`aws-ap-southeast-1`) to match the `sin1` function region in `vercel.json`, since every API call makes several round trips to it. Add Neon from the project's Storage tab (it sets the variable for you), or paste a connection string into the project's environment variables.
3. Deploy, or push to the production branch. The first request runs the migrations.

## API

| Method | Path | Body | Answer |
|---|---|---|---|
| `GET` | `/api/threads?bbox=w,s,e,n` | | `{ threads: [{ id, lat, lng, preview, messages, expires_at }] }`: live threads in view, the most recently extended first, at most 500 |
| `GET` | `/api/threads/{id}` | | the thread and its messages, or 404 once it has expired |
| `POST` | `/api/threads` | `{ lat, lng, name, body }` | 201 and the new thread |
| `POST` | `/api/threads/{id}/messages` | `{ name, body, lat, lng }` | 201 and the thread; 403 if the poster is over 100 km away; 404 if it has expired |

Posting needs the `X-Champt-Author` header, a UUID. A thread tells each reader which messages are theirs (`mine`), whether their reply would reset the clock (`reply_extends`), and how far away a reply may come from (`reply_radius_km`); author ids never leave the server. Names run up to 32 characters and messages up to 500. Errors come back as `{ error }`.
