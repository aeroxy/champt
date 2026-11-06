import { useCallback, useEffect, useRef, useState } from 'react'
import { LocateFixed, Plus, RefreshCw } from 'lucide-react'
import { type Pin, type Thread, ApiError, api, problem, saveName, savedName } from './api'
import { type Focus, MapView, type Spot } from './MapView'
import { ComposePanel, PickPanel, ThreadPanel, type View } from './Panel'
import { Button, Chip, LiveChip } from './ui'

/** Threads are pinned to a 0.001° grid (about 100 m); the server snaps too. */
const snap = (x: number) => Math.round(x * 1000) / 1000

type Location = 'locating' | 'on' | 'off'

export function App() {
  const now = useNow(30_000)
  const [pins, setPins] = useState<Pin[]>([])
  const [loading, setLoading] = useState(false)
  const [view, setView] = useState<View>({ kind: 'none' })
  const [thread, setThread] = useState<Thread | null>(null)
  const [threadError, setThreadError] = useState<string | null>(null)
  const [me, setMe] = useState<Spot | null>(null)
  const [location, setLocation] = useState<Location>('locating')
  const [focus, setFocus] = useState<Focus | null>(null)
  const [name, setName] = useState(savedName)
  const bbox = useRef<string | null>(null)
  const pinsRequest = useRef<AbortController | null>(null)

  // Follow the person's position, and fly there the first time it arrives.
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
          setFocus({ spot, zoom: 15, key: Date.now() })
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

  const openThread = useCallback(async (id: number) => {
    setView({ kind: 'thread', id })
    setThread((shown) => (shown?.id === id ? shown : null))
    setThreadError(null)
    try {
      setThread(await api.thread(id))
    } catch (e) {
      setThread(null)
      setThreadError(problem(e))
    }
  }, [])

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

  function rename(next: string) {
    setName(next)
    saveName(next)
  }

  const author = () => name.trim() || 'Anonymous'

  async function start(body: string) {
    if (!me) throw new ApiError('Your location is not known yet.', 0)
    const created = await api.start({ lat: snap(me.lat), lng: snap(me.lng), name: author(), body })
    setThread(created)
    setThreadError(null)
    setView({ kind: 'thread', id: created.id })
    loadPins()
  }

  async function reply(body: string) {
    if (view.kind !== 'thread') return
    try {
      setThread(await api.reply(view.id, { name: author(), body }))
      loadPins()
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) {
        setThread(null)
        setThreadError(e.message)
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

        {view.kind !== 'compose' && (
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
        )}
      </header>

      {view.kind === 'pick' && <PickPanel pins={picked} now={now} onOpen={openThread} onClose={close} />}
      {view.kind === 'thread' && (
        <ThreadPanel
          thread={thread}
          error={threadError}
          now={now}
          name={name}
          onName={rename}
          onReply={reply}
          onClose={close}
        />
      )}
      {view.kind === 'compose' && (
        <ComposePanel canPost={me !== null} name={name} onName={rename} onStart={start} onClose={close} />
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
