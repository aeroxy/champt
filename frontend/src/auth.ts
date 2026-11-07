// Sign in with Aero. Identity only: Aero's consent screen sends the browser
// back to /auth/aero with an encrypted token that only Aero and Champt's API
// can open. The page keeps it as an opaque string, sends it with each call,
// and learns who is signed in from the API (/api/me).

const TOKEN_KEY = 'champt.session'
const STATE_KEY = 'champt.sign-in-state'

export type Session = { token: string }

// Storage can throw (private windows, blocked site data). Signing in then
// lasts only as long as the page.
function read(storage: () => Storage, key: string): string | null {
  try {
    return storage().getItem(key)
  } catch {
    return null
  }
}

function write(storage: () => Storage, key: string, value: string | null) {
  try {
    if (value === null) storage().removeItem(key)
    else storage().setItem(key, value)
  } catch {
    // Keep going without it.
  }
}

const local = () => localStorage
const tab = () => sessionStorage

/** The current sign-in, if any. Only the API can tell whether it still holds. */
export function currentSession(): Session | null {
  const token = read(local, TOKEN_KEY)
  return token ? { token } : null
}

/**
 * Leaves for Aero's consent screen, by way of the API, which holds Champt's
 * client id and sends the browser on. `state` stays here, to check the
 * answer against.
 */
export function signIn() {
  const state = crypto.randomUUID()
  write(tab, STATE_KEY, state)
  location.assign(`/api/sign-in?state=${state}`)
}

const listeners = new Set<() => void>()

/** Hears every end of a sign-in: signing out, or the API refusing the token. */
export function onSignOut(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function signOut() {
  write(local, TOKEN_KEY, null)
  listeners.forEach((listener) => listener())
}

/**
 * On Aero's callback, keeps the token if the answer belongs to this browser's
 * own sign-in (its `state` matches), then takes it out of the address bar.
 * The check stops someone pushing their own token into a victim's browser.
 */
export function finishSignIn() {
  if (location.pathname !== '/auth/aero') return
  const answer = new URLSearchParams(location.hash.slice(1))
  const expected = read(tab, STATE_KEY)
  write(tab, STATE_KEY, null)
  const token = answer.get('aero_identity')
  if (token && expected && answer.get('state') === expected) {
    write(local, TOKEN_KEY, token)
  }
  history.replaceState(null, '', '/')
}
