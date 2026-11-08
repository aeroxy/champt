import { type FormEvent, useCallback, useEffect, useRef, useState } from 'react'
import { LocateFixed, LogIn, Plus, RefreshCw } from 'lucide-react'
import { type Me, type Pin, type Post, type Thread, ApiError, api, problem } from './api'
import { type Session, currentSession, onSignOut, signIn, signOut } from './auth'
import { linkedThread, threadPath } from './link'
import { type Focus, MapView, type Spot } from './MapView'
import { ComposePanel, PickPanel, ThreadPanel, type ThreadError, type View } from './Panel'
import { Avatar, Button, Chip, LiveChip, TextButton } from './ui'

/** Threads are pinned to a 0.001° grid (about 100 m); the server snaps too. */
const snap = (x: number) => Math.round(x * 1000) / 1000

type Location = 'locating' | 'on' | 'off'

export function App() {
  const now = useNow(30_000)
  const [pins, setPins] = useState<Pin[]>([])
  const [loading, setLoading] = useState(false)
  // A thread's link opens the page on that thread.
  const [view, setView] = useState<View>(() => {
    const id = linkedThread(window.location.pathname)
    return id === null ? { kind: 'none' } : { kind: 'thread', id }
  })
  const [thread, setThread] = useState<Thread | null>(null)
  const [threadError, setThreadError] = useState<ThreadError | null>(null)
  const [me, setMe] = useState<Spot | null>(null)
  const [location, setLocation] = useState<Location>('locating')
  const [focus, setFocus] = useState<Focus | null>(null)
  const [session, setSession] = useState<Session | null>(currentSession)
  const bbox = useRef<string | null>(null)
  /** Whether the map has gone to a linked thread, so your position doesn't pull it away. */
  const atLink = useRef(false)

  /** Who the API says is signed in; null while unknown. */
  const [account, setAccount] = useState<Me | null>(null)
  const [naming, setNaming] = useState(false)

  // Signing out, or the API refusing the token, ends the session here too.
  useEffect(() => onSignOut(() => setSession(null)), [])

  // Only the API can open the token, so ask it who this is. A refused token
  // signs out on the way (see api.ts).
  useEffect(() => {
    setAccount(null)
    setNaming(false)
    if (!session) return
    let current = true
    api
      .me()
      .then((me) => current && setAccount(me))
      .catch(() => {})
    return () => {
      current = false
    }
  }, [session])
  const pinsRequest = useRef<AbortController | null>(null)

  // Follow the person's position, and fly there the first time it arrives,
  // unless a link has already taken the map to its thread.
  useEffect(() => {
    if (!('geolocation' in navigator)) {
      setLocation('off')
      return
    }
    let first = true
    const watch = navigator.geolocation.watchPosition(
      (position) => {
        const spot = { lat: position.coords.latitude, lng: position.coords.longitude }
        setMe(spot)
        setLocation('on')
        if (first) {
          first = false
          if (!atLink.current) setFocus({ spot, zoom: 15, key: Date.now() })
        }
      },
      // A later failure keeps the last good position.
      () => setLocation((was) => (was === 'on' ? 'on' : 'off')),
      { enableHighAccuracy: true, maximumAge: 30_000 },
    )
    return () => navigator.geolocation.clearWatch(watch)
  }, [])

  const loadPins = useCallback(async () => {
    if (!bbox.current) return
    pinsRequest.current?.abort()
    const request = new AbortController()
    pinsRequest.current = request
    setLoading(true)
    try {
      setPins(await api.pins(bbox.current, request.signal))
    } catch (e) {
      // Keep the pins already shown; a newer request may have replaced this one.
      if (!request.signal.aborted) console.error(e)
    } finally {
      if (pinsRequest.current === request) setLoading(false)
    }
  }, [])

  const onView = useCallback(
    (next: string) => {
      bbox.current = next
      loadPins()
    },
    [loadPins],
  )

  /** Shows a thread, and gives it back once loaded; null if it's gone or didn't load. */
  const openThread = useCallback(async (id: number): Promise<Thread | null> => {
    setView({ kind: 'thread', id })
    setThread((shown) => (shown?.id === id ? shown : null))
    setThreadError(null)
    try {
      const loaded = await api.thread(id)
      setThread(loaded)
      return loaded
    } catch (e) {
      setThread(null)
      setThreadError({ text: problem(e), gone: e instanceof ApiError && e.status === 404 })
      return null
    }
  }, [])

  // A thread's link opens that thread, and takes the map to it.
  useEffect(() => {
    const id = linkedThread(window.location.pathname)
    if (id === null) return
    openThread(id).then((shown) => {
      if (!shown) return
      atLink.current = true
      setFocus({ spot: { lat: shown.lat, lng: shown.lng }, zoom: 15, key: Date.now() })
    })
  }, [openThread])

  // The address bar shows the open thread's link, so it can be shared as it
  // is, and a sign-in comes back to it.
  useEffect(() => {
    const path = view.kind === 'thread' ? threadPath(view.id) : '/'
    if (window.location.pathname !== path) history.replaceState(null, '', path)
  }, [view])

  const onPick = useCallback(
    (ids: number[]) => (ids.length === 1 ? openThread(ids[0]) : setView({ kind: 'pick', ids })),
    [openThread],
  )

  const close = useCallback(() => setView({ kind: 'none' }), [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && close()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [close])

  function refresh() {
    loadPins()
    if (view.kind === 'thread') openThread(view.id)
  }

  async function start(post: Post) {
    if (!me) throw new ApiError('Your location is not known yet.', 0)
    const created = await api.start({ ...post, lat: snap(me.lat), lng: snap(me.lng) })
    setThread(created)
    setThreadError(null)
    setView({ kind: 'thread', id: created.id })
    loadPins()
  }

  async function reply(post: Post) {
    if (view.kind !== 'thread') return
    if (!me) throw new ApiError('Your location is not known yet.', 0)
    try {
      setThread(await api.reply(view.id, { ...post, lat: snap(me.lat), lng: snap(me.lng) }))
      loadPins()
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) {
        setThread(null)
        setThreadError({ text: e.message, gone: true })
        loadPins()
        return
      }
      throw e
    }
  }

  const selected = view.kind === 'thread' ? view.id : null
  const picked = view.kind === 'pick' ? pins.filter((pin) => view.ids.includes(pin.id)) : []

  return (
    <div className="relative h-dvh w-full overflow-hidden bg-paper">
      <MapView pins={pins} now={now} me={me} selected={selected} focus={focus} onPick={onPick} onView={onView} />

      <header className="absolute top-4 right-4 left-4 z-10 rounded-lg border border-border bg-surface/95 p-4 shadow-md backdrop-blur md:right-auto md:w-[360px]">
        <div className="flex items-center gap-3">
          <img src="/champt.svg" alt="" className="size-10 shrink-0" />
          <div className="min-w-0 flex-1">
            <h1 className="text-[28px] leading-[0.95] font-extrabold tracking-[-0.04em] text-fg">Champt</h1>
            <p className="eyebrow mt-1.5 truncate">Local threads</p>
          </div>
          <Button
            variant="secondary"
            square
            aria-label="Refresh"
            title="Refresh"
            onClick={refresh}
          >
            <RefreshCw className={`size-4 ${loading ? 'animate-spin' : ''}`} />
          </Button>
          <Button
            variant="secondary"
            square
            aria-label="Go to my location"
            title="Go to my location"
            disabled={!me}
            onClick={() => me && setFocus({ spot: me, zoom: 15, key: Date.now() })}
          >
            <LocateFixed className="size-4" />
          </Button>
        </div>

        <p className="mt-3 hidden text-[13px] text-fg-2 md:block">
          Chat with your local geeks. Threads are pinned to where people are, and each one goes after 24 hours
          unless someone else replies.
        </p>

        <div className="mt-3 flex flex-wrap items-center gap-2">
          {pins.length > 0 ? (
            <LiveChip>
              {pins.length} live in view
            </LiveChip>
          ) : (
            <Chip>None in view</Chip>
          )}
          {location === 'locating' && <Chip>Finding you…</Chip>}
          {location === 'off' && <Chip>Location off</Chip>}
        </div>

        {session ? (
          <>
            {account && (account.nickname === null || naming) ? (
              <>
                <NicknameForm
                  current={account.nickname}
                  onSaved={(next) => {
                    setAccount(next)
                    setNaming(false)
                  }}
                  onClose={account.nickname ? () => setNaming(false) : undefined}
                />
                {account.nickname && <GravatarSwitch account={account} onSaved={setAccount} />}
              </>
            ) : (
              view.kind !== 'compose' && (
              <Button
                variant="accent"
                className="mt-3 w-full"
                disabled={!me}
                title={me ? undefined : 'Allow location access to start a thread'}
                onClick={() => setView({ kind: 'compose' })}
              >
                <Plus className="size-4" />
                Start a thread here
              </Button>
              )
            )}
            <p className="mt-3 flex min-w-0 items-center gap-1.5 text-[12px] text-fg-3">
              <Avatar src={account?.avatar ?? null} className="size-4" />
              <span className="truncate">
                {account?.nickname ? (
                  <>
                    Signed in as <span className="font-mono text-fg-2">{account.nickname}</span>
                  </>
                ) : (
                  'Signed in'
                )}
              </span>
              {account?.nickname && !naming && (
                <>
                  <span aria-hidden>·</span>
                  <TextButton onClick={() => setNaming(true)}>Edit</TextButton>
                </>
              )}
              <span aria-hidden>·</span>
              <TextButton onClick={signOut}>Sign out</TextButton>
            </p>
          </>
        ) : (
          <Button variant="accent" className="mt-3 w-full" onClick={signIn}>
            <LogIn className="size-4" />
            Sign in with Aero
          </Button>
        )}
      </header>

      {view.kind === 'pick' && <PickPanel pins={picked} now={now} onOpen={openThread} onClose={close} />}
      {view.kind === 'thread' && (
        <ThreadPanel
          thread={thread}
          error={threadError}
          now={now}
          me={me}
          signedIn={session !== null}
          nickname={account?.nickname ?? null}
          onReply={reply}
          onClose={close}
        />
      )}
      {view.kind === 'compose' && (
        <ComposePanel
          canPost={me !== null}
          signedIn={session !== null}
          nickname={account?.nickname ?? null}
          onStart={start}
          onClose={close}
        />
      )}
    </div>
  )
}

/** Choosing or changing a nickname: the name others see instead of an email. */
function NicknameForm({
  current,
  onSaved,
  onClose,
}: {
  current: string | null
  onSaved: (me: Me) => void
  onClose?: () => void
}) {
  const [text, setText] = useState(current ?? '')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // The API's rules, so Save only lights up for a name it could accept.
  const fits = /^[A-Za-z0-9_]{3,20}$/.test(text.trim())

  async function save(event: FormEvent) {
    event.preventDefault()
    setBusy(true)
    setError(null)
    try {
      onSaved(await api.setNickname(text.trim()))
    } catch (e) {
      setError(problem(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <form onSubmit={save} className="mt-3">
      <label htmlFor="nickname" className="eyebrow">
        {current ? 'Change your nickname' : 'Choose a nickname'}
      </label>
      <div className="mt-1.5 flex gap-2">
        <input
          id="nickname"
          value={text}
          onChange={(e) => setText(e.target.value)}
          maxLength={20}
          autoFocus
          autoComplete="username"
          spellCheck={false}
          placeholder="ada_l"
          className="h-10 min-w-0 flex-1 rounded-md border border-border bg-surface px-3 font-mono text-sm text-fg transition duration-200 ease-out placeholder:text-fg-3 focus:border-action focus:shadow-focus focus:outline-none"
        />
        <Button type="submit" variant="accent" disabled={busy || !fits}>
          {busy ? 'Saving…' : 'Save'}
        </Button>
        {onClose && (
          <Button type="button" variant="ghost" onClick={onClose}>
            Close
          </Button>
        )}
      </div>
      <p className="mt-1.5 text-[12px] text-fg-3">
        3–20 English letters, numbers or _. Others see it instead of your email.
      </p>
      {error && (
        <p role="alert" className="mt-2 rounded-sm bg-danger-soft px-3 py-2 text-[12px] text-danger-strong">
          {error}
        </p>
      )}
    </form>
  )
}

/**
 * Showing a Gravatar next to your posts, or not. It saves at once. Champt
 * fetches the picture itself, so no browser sees the hash of your email.
 */
function GravatarSwitch({ account, onSaved }: { account: Me; onSaved: (me: Me) => void }) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const shown = account.avatar !== null

  async function toggle() {
    setBusy(true)
    setError(null)
    try {
      onSaved(await api.setGravatar(!shown))
    } catch (e) {
      setError(problem(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="mt-3 border-t border-border pt-3">
      <div className="flex items-center gap-2">
        <label className="flex min-w-0 flex-1 items-center gap-2 text-[13px] font-medium text-fg">
          <input
            type="checkbox"
            checked={shown}
            disabled={busy}
            onChange={toggle}
            className="size-4 shrink-0 accent-action"
          />
          Show my Gravatar next to my posts
        </label>
        <Avatar
          src={account.avatar}
          className="size-6"
          missing={<span className="shrink-0 text-[12px] text-fg-3">None found</span>}
        />
      </div>
      <p className="mt-1.5 text-[12px] text-fg-3">
        Champt fetches it for you, so your email stays private. People may still recognise the picture if you use
        it elsewhere.
      </p>
      {error && (
        <p role="alert" className="mt-2 rounded-sm bg-danger-soft px-3 py-2 text-[12px] text-danger-strong">
          {error}
        </p>
      )}
    </div>
  )
}

/** The current time, updated every `ms` so "time left" labels keep moving. */
function useNow(ms: number) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), ms)
    return () => clearInterval(timer)
  }, [ms])
  return now
}
