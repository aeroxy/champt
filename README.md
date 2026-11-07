# Champt

Chat threads pinned to places on a map. You start a thread where you are. Anyone can read it from anywhere, and anyone within 100 km of it can reply. A thread disappears after 24 hours unless someone *else* replies: a reply resets the clock only when its author is not the one who posted last, so it takes two people taking turns to keep a thread alive.

An early 2013 prototype (Node and Socket.IO, live dots on a map) is on the `archive` branch.

## How it behaves

| Rule | Where it lives |
|---|---|
| A thread starts only where you are, snapped to a 0.001° grid (about 100 m) | `snap` in `src/geo.rs`; the browser rounds before sending, too |
| A thread lives 24 hours past its last turn change | one `UPDATE` in `add_message`, `src/lib.rs`; every read filters on `expires_at > now()` |
| The map pans worldwide, the antimeridian included, and any thread can be read | `list_threads`, `BBox` in `src/geo.rs` |
| Replies come only from within 100 km of a thread. The poster's position travels with the reply for the check and is not stored | `REPLY_RADIUS_KM` and `add_message` in `src/lib.rs`; the reply box in `frontend/src/Panel.tsx` |
| Reading needs no account. Posting needs Sign in with Aero and a nickname: 3–20 English letters, numbers or `_`, unique ignoring case, and all anyone else sees unless they also show their Gravatar. The email stays on the server, in `people`, for audits | `src/auth.rs`, `Author` and `valid_nickname` in `src/lib.rs`, `frontend/src/auth.ts` |
| A Gravatar shows next to someone's posts only while they choose to. The API fetches it and serves it under a random id of its own, so no browser sees a hash of the email, and hiding it retires the id | `src/gravatar.rs`, `set_gravatar` and `avatar` in `src/lib.rs`, the switch in `frontend/src/App.tsx` |
| No live updates: Refresh reloads the pins and the open thread | `frontend/src/App.tsx` |

Expired threads are deleted whenever someone starts a new one, so nothing runs on a schedule.

## What runs where

| Piece | Choice |
|---|---|
| API | Rust: axum 0.8 and sqlx with raw SQL, as one Vercel Function (`api/server.rs`) through `vercel_runtime`'s axum layer. Vercel's Rust runtime is in beta. |
| Database | Postgres. Neon works through either its pooled or its direct connection string. Migrations in `migrations/` run when a function instance starts. |
| Front end | React 19, Vite and Tailwind 4, built with bun and served from Vercel's CDN |
| Map | MapLibre GL with OpenFreeMap's `positron` tiles: no key, no account |
| Avatars | Gravatar, for those who turn it on. The API asks for the picture with `d=404`, so a missing one stays missing, and gets only pictures rated G. Browsers may keep it for an hour; shared caches keep none (`src/gravatar.rs`) |
| Analytics | Google Analytics, production builds only. It starts after a sign-in's token has left the address bar, and is given the page address without query or fragment, so no token can reach it (`frontend/src/analytics.ts`) |
| Sign-in | [Aero](https://aero.oops.wtf)'s Login with Aero: identity only, not OAuth. The token is PASETO `v4.local`: the visitor's email, encrypted with a key only Aero and Champt hold, valid for 30 days. The browser keeps it without being able to read it; the API decrypts it and checks its audience and times on every call that needs to know who is asking. The client id and key both live in the backend's environment (`AERO_CLIENT_ID`, `AERO_LOGIN_KEY`); the page starts a sign-in at `/api/sign-in`, which adds the client id. |

## Local development

```sh
cp .env.example .env          # set DATABASE_URL
cargo run --bin dev           # the API on http://127.0.0.1:8787
cd frontend && bun install && bun run dev   # http://localhost:5176, proxying /api
```

Browsers only share location with secure pages. `localhost` counts; a LAN address over plain http does not.

A sign-in can only finish on the callback registered with Aero, which is the production site, so locally you can read but not post. To post locally, put the production `AERO_CLIENT_ID` and `AERO_LOGIN_KEY` in `.env` and copy `champt.session` from the production site's localStorage into localhost's: the token is addressed to Champt, not to a domain.

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
3. Register Champt on [Aero's Connect page](https://aero.oops.wtf/connect) under Login with Aero, with the callback `https://<your domain>/auth/aero`. It gives a client id and a `k4.local.…` key, the key shown only once: set them as `AERO_CLIENT_ID` and `AERO_LOGIN_KEY` in the project's environment variables, never in the repo, and redeploy. Sign-ins work once Aero approves the registration. Until both are set, signing in answers "Sign-in isn't set up yet" and nobody can post. A lost key means deleting the app on Aero and registering again, which gives a new id and key.
4. Deploy, or push to the production branch. The first request runs the migrations.

## API

| Method | Path | Body | Answer |
|---|---|---|---|
| `GET` | `/api/threads?bbox=w,s,e,n` | | `{ threads: [{ id, lat, lng, preview, messages, expires_at }] }`: live threads in view, the most recently extended first, at most 500 |
| `GET` | `/api/threads/{id}` | | the thread and its messages, or 404 once it has expired |
| `POST` | `/api/threads` | `{ lat, lng, body }` | 201 and the new thread; 403 without a nickname |
| `POST` | `/api/threads/{id}/messages` | `{ body, lat, lng }` | 201 and the thread; 403 without a nickname or from over 100 km away; 404 if it has expired |
| `GET` | `/api/sign-in?state=…` | | 303 to Aero's consent screen with Champt's client id and a 30-day `ttl`; 400 unless `state` is 1–512 letters, digits, `-` or `_`; 503 before registration |
| `GET` | `/api/me` | | `{ nickname, avatar }` of whoever is signed in: `nickname` is null until they choose one, `avatar` is a link to their Gravatar while they show it; 401 when signed out. The page can't open its own token |
| `PUT` | `/api/me` | `{ nickname }` | 200 and `{ nickname, avatar }`; 400 if it breaks the rules; 409 if someone has it. Changing it frees the old one |
| `PUT` | `/api/me/gravatar` | `{ show }` | 200 and `{ nickname, avatar }`; 403 without a nickname. Showing it makes a new random link, hiding it retires the link |
| `GET` | `/api/avatars/{id}` | | the picture, fetched from Gravatar; 404 once its owner hides it, or if Gravatar has none; 503 if Gravatar doesn't answer |

Posting needs `Authorization: Bearer <token>` with a token from Sign in with Aero. Without one, or with one that fails its checks, the answer is 401; so is a thread read with a failing token, which tells the page to sign in again. A thread tells each reader which messages are theirs (`mine`), whether their reply would reset the clock (`reply_extends`), and how far away a reply may come from (`reply_radius_km`); author ids, emails and anything derived from them never leave the server. A message shows its author's current nickname, and `avatar`, a link to their Gravatar while they show it. Messages run up to 500 characters. Errors come back as `{ error }`.

## Auditing

Who wrote what, with their Aero email, from Neon's SQL editor:

```sql
SELECT m.created_at, p.nickname, p.email, m.body
  FROM messages m
  JOIN people p ON p.id = m.author_id
 ORDER BY m.created_at DESC;
```

Messages go with their threads. `people` keeps each person's email and nickname, which is what holds a nickname for them.
