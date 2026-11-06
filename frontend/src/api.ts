// Talks to the Rust API under /api. Calls carry the Aero sign-in, when there
// is one, as a bearer token, and the API decides who that is.

import { currentSession, signOut } from './auth'

export type Pin = {
  id: number
  lat: number
  lng: number
  preview: string
  messages: number
  expires_at: string
}

export type Message = {
  id: number
  author_name: string
  body: string
  created_at: string
  mine: boolean
}

export type Thread = {
  id: number
  lat: number
  lng: number
  created_at: string
  expires_at: string
  /** Whether a reply from this browser would reset the clock. */
  reply_extends: boolean
  /** Replies must come from within this distance of the thread. */
  reply_radius_km: number
  messages: Message[]
}

/** The signed-in person as others see them. Posting needs a nickname. */
export type Me = { nickname: string | null }

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message)
  }
}

type Options = { body?: unknown; method?: 'POST' | 'PUT'; signal?: AbortSignal }

async function call<T>(path: string, options: Options = {}): Promise<T> {
  const posting = options.body !== undefined
  const session = currentSession()
  const response = await fetch(path, {
    method: posting ? (options.method ?? 'POST') : 'GET',
    headers: {
      ...(session ? { Authorization: `Bearer ${session.token}` } : {}),
      ...(posting ? { 'Content-Type': 'application/json' } : {}),
    },
    body: posting ? JSON.stringify(options.body) : undefined,
    signal: options.signal,
  })
  if (response.status === 401 && session) {
    // The API refused the sign-in, so forget it. A read works signed out and
    // goes again without it; a post tells the person to sign in again.
    signOut()
    if (!posting) return call<T>(path, options)
  }
  const data = await response.json().catch(() => null)
  if (!response.ok) {
    throw new ApiError(data?.error ?? `The server answered ${response.status}.`, response.status)
  }
  return data as T
}

export const api = {
  /** Live threads inside `bbox` (west,south,east,north). */
  pins: (bbox: string, signal?: AbortSignal) =>
    call<{ threads: Pin[] }>(`/api/threads?bbox=${bbox}`, { signal }).then((list) => list.threads),
  thread: (id: number) => call<Thread>(`/api/threads/${id}`),
  /** Who is signed in, as others see them: the page can't open the token itself. */
  me: () => call<Me>('/api/me'),
  /** Chooses or changes the nickname; 409 if someone has it. */
  setNickname: (nickname: string) => call<Me>('/api/me', { body: { nickname }, method: 'PUT' }),
  start: (thread: { lat: number; lng: number; body: string }) => call<Thread>('/api/threads', { body: thread }),
  /** `lat`/`lng` is where the poster is, checked against the reply radius and not stored. */
  reply: (id: number, message: { body: string; lat: number; lng: number }) =>
    call<Thread>(`/api/threads/${id}/messages`, { body: message }),
}

/** What to show the person when a request fails. */
export function problem(error: unknown): string {
  if (error instanceof ApiError) return error.message
  return 'Could not reach Champt. Check your connection and try again.'
}
