// Talks to the Rust API under /api. Until there is a login, a random id kept
// in this browser (sent as X-Champt-Author) is what tells one person from
// another, and so decides whether a reply resets a thread's clock.

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

const ID_KEY = 'champt.author'
const NAME_KEY = 'champt.name'
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

// Storage can throw (private windows, blocked site data). Without it the id
// lasts for this page load only, which is still enough to post.
function read(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value)
  } catch {
    // Keep going without it.
  }
}

const authorId = (() => {
  const stored = read(ID_KEY)
  if (stored && UUID.test(stored)) return stored
  const id = crypto.randomUUID()
  write(ID_KEY, id)
  return id
})()

export const savedName = () => read(NAME_KEY) ?? ''
export const saveName = (name: string) => write(NAME_KEY, name)

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message)
  }
}

async function call<T>(path: string, options: { body?: unknown; signal?: AbortSignal } = {}): Promise<T> {
  const posting = options.body !== undefined
  const response = await fetch(path, {
    method: posting ? 'POST' : 'GET',
    headers: {
      'X-Champt-Author': authorId,
      ...(posting ? { 'Content-Type': 'application/json' } : {}),
    },
    body: posting ? JSON.stringify(options.body) : undefined,
    signal: options.signal,
  })
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
  start: (thread: { lat: number; lng: number; name: string; body: string }) =>
    call<Thread>('/api/threads', { body: thread }),
  /** `lat`/`lng` is where the poster is, checked against the reply radius and not stored. */
  reply: (id: number, message: { name: string; body: string; lat: number; lng: number }) =>
    call<Thread>(`/api/threads/${id}/messages`, { body: message }),
}

/** What to show the person when a request fails. */
export function problem(error: unknown): string {
  if (error instanceof ApiError) return error.message
  return 'Could not reach Champt. Check your connection and try again.'
}
