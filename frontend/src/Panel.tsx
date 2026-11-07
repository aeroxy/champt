import { type FormEvent, type ReactNode, useEffect, useRef, useState } from 'react'
import { X } from 'lucide-react'
import { type Pin, type Thread, problem } from './api'
import { signIn } from './auth'
import { ago, distanceKm, timeLeft } from './format'
import type { Spot } from './MapView'
import { Avatar, Button, LiveChip } from './ui'

export type View =
  | { kind: 'none' }
  | { kind: 'pick'; ids: number[] }
  | { kind: 'thread'; id: number }
  | { kind: 'compose' }

/** Side panel on wide screens, bottom sheet on phones. */
function Shell({ label, onClose, children }: { label: ReactNode; onClose: () => void; children: ReactNode }) {
  return (
    <aside
      aria-label="Thread"
      className={
        'absolute inset-x-0 bottom-0 z-20 flex max-h-[72dvh] flex-col overflow-hidden rounded-t-xl ' +
        'border border-border bg-surface shadow-lg md:inset-x-auto md:top-4 md:right-4 md:bottom-auto ' +
        'md:max-h-[calc(100dvh-2rem)] md:w-[400px] md:rounded-xl'
      }
    >
      <div className="flex min-h-14 items-center gap-3 border-b border-border px-5">
        <div className="flex min-w-0 flex-1 items-center gap-2">{label}</div>
        <Button variant="ghost" square className="-mr-2" aria-label="Close" title="Close (Esc)" onClick={onClose}>
          <X className="size-4" />
        </Button>
      </div>
      {children}
    </aside>
  )
}

export function PickPanel({
  pins,
  now,
  onOpen,
  onClose,
}: {
  pins: Pin[]
  now: number
  onOpen: (id: number) => void
  onClose: () => void
}) {
  return (
    <Shell label={<span className="eyebrow">{pins.length} threads here</span>} onClose={onClose}>
      <ul className="min-h-0 overflow-y-auto">
        {pins.map((pin) => (
          <li key={pin.id} className="border-b border-border last:border-b-0">
            <button
              className="w-full px-5 py-4 text-left transition duration-200 ease-out hover:bg-sunken focus-visible:bg-sunken focus-visible:outline-none"
              onClick={() => onOpen(pin.id)}
            >
              <p className="line-clamp-2 text-sm text-fg">{pin.preview}</p>
              <p className="eyebrow mt-1.5 tabular">
                {pin.messages} {pin.messages === 1 ? 'message' : 'messages'} · {timeLeft(pin.expires_at, now)}
              </p>
            </button>
          </li>
        ))}
      </ul>
    </Shell>
  )
}

export function ThreadPanel({
  thread,
  error,
  now,
  me,
  signedIn,
  nickname,
  onReply,
  onClose,
}: {
  thread: Thread | null
  error: string | null
  now: number
  me: Spot | null
  signedIn: boolean
  /** Null until the signed-in person chooses one. */
  nickname: string | null
  onReply: (body: string) => Promise<void>
  onClose: () => void
}) {
  const end = useRef<HTMLLIElement>(null)
  const count = thread?.messages.length ?? 0
  useEffect(() => {
    end.current?.scrollIntoView({ block: 'end' })
  }, [thread?.id, count])

  const label = thread ? (
    <>
      <span className="eyebrow truncate">
        {count} {count === 1 ? 'message' : 'messages'}
      </span>
      <LiveChip>{timeLeft(thread.expires_at, now)}</LiveChip>
    </>
  ) : (
    <span className="eyebrow">Thread</span>
  )

  return (
    <Shell label={label} onClose={onClose}>
      {error ? (
        <div className="px-5 py-10 text-center">
          <p className="text-[18px] font-bold text-fg">{error}</p>
          <p className="mt-1 text-[13px] text-fg-2">A thread goes after 24 hours unless someone else replies.</p>
        </div>
      ) : !thread ? (
        <p className="eyebrow px-5 py-10 text-center">Loading…</p>
      ) : (
        <>
          <ol className="min-h-0 flex-1 space-y-5 overflow-y-auto px-5 py-5">
            {thread.messages.map((message, i) => (
              <li key={message.id} ref={i === count - 1 ? end : undefined}>
                <div className="flex items-baseline gap-2">
                  <Avatar src={message.avatar} className="size-5 self-center" />
                  <span className="truncate text-[13px] font-semibold text-fg">{message.author_name}</span>
                  {message.mine && (
                    <span className="rounded-full border border-border px-1.5 text-[10px] font-medium text-fg-3">
                      you
                    </span>
                  )}
                  <time dateTime={message.created_at} className="eyebrow ml-auto shrink-0 text-[10px] tabular">
                    {ago(message.created_at, now)}
                  </time>
                </div>
                <p className="mt-1 text-sm break-words whitespace-pre-wrap text-fg-2">{message.body}</p>
              </li>
            ))}
          </ol>
          <ReplyArea thread={thread} me={me} signedIn={signedIn} nickname={nickname}>
            <Composer
              nickname={nickname ?? ''}
              placeholder="Reply…"
              submit="Reply"
              variant="primary"
              hint={
                thread.reply_extends
                  ? 'Your reply resets the clock to 24 hours.'
                  : 'You posted last. The clock resets only when someone else replies.'
              }
              onSubmit={onReply}
            />
          </ReplyArea>
        </>
      )}
    </Shell>
  )
}

/** The reply box when the thread is within reach; otherwise why it is read only. */
function ReplyArea({
  thread,
  me,
  signedIn,
  nickname,
  children,
}: {
  thread: Thread
  me: Spot | null
  signedIn: boolean
  nickname: string | null
  children: ReactNode
}) {
  const radius = thread.reply_radius_km
  const away = me ? distanceKm(me, thread) : null
  if (away !== null && away <= radius) {
    return (
      <AccountGate signedIn={signedIn} nickname={nickname} to="reply">
        {children}
      </AccountGate>
    )
  }
  return (
    <div className="border-t border-border bg-sunken px-5 py-4">
      <p className="eyebrow">Read only</p>
      <p className="mt-1 text-[13px] text-fg-2">
        {away === null
          ? `Allow location access to reply. You can reply to threads within ${radius} km of you.`
          : `This thread is ${Math.round(away)} km away. You can reply to threads within ${radius} km of you.`}
      </p>
    </div>
  )
}

/** What stands between someone and posting: a sign-in, then a nickname. */
function AccountGate({
  signedIn,
  nickname,
  to,
  children,
}: {
  signedIn: boolean
  nickname: string | null
  to: string
  children: ReactNode
}) {
  if (!signedIn) return <SignInPrompt to={to} />
  if (!nickname) {
    return (
      <p className="border-t border-border bg-sunken px-5 py-4 text-[13px] text-fg-2">
        Choose a nickname at the top to {to}. It is what others see, instead of your email.
      </p>
    )
  }
  return <>{children}</>
}

/** Where a reply box would be, for someone who isn't signed in. */
function SignInPrompt({ to }: { to: string }) {
  return (
    <div className="flex items-center gap-3 border-t border-border bg-sunken px-5 py-4">
      <p className="min-w-0 flex-1 text-[13px] text-fg-2">Sign in with Aero to {to}.</p>
      <Button onClick={signIn}>Sign in</Button>
    </div>
  )
}

export function ComposePanel({
  canPost,
  signedIn,
  nickname,
  onStart,
  onClose,
}: {
  canPost: boolean
  signedIn: boolean
  nickname: string | null
  onStart: (body: string) => Promise<void>
  onClose: () => void
}) {
  return (
    <Shell label={<span className="eyebrow">New thread · where you are</span>} onClose={onClose}>
      <p className="px-5 py-4 text-[13px] text-fg-2">
        It is pinned within about 100 m of you, and anyone can read it. It disappears after 24 hours unless someone
        else replies.
      </p>
      <AccountGate signedIn={signedIn} nickname={nickname} to="start a thread">
        {canPost ? (
          <Composer
            nickname={nickname ?? ''}
            placeholder="What's happening here?"
            submit="Post"
            variant="accent"
            autoFocus
            onSubmit={onStart}
          />
        ) : (
          <p className="border-t border-border bg-sunken px-5 py-4 text-[13px] text-fg-2">
            Allow location access to start a thread where you are.
          </p>
        )}
      </AccountGate>
    </Shell>
  )
}

function Composer({
  nickname,
  placeholder,
  submit,
  variant,
  hint,
  autoFocus,
  onSubmit,
}: {
  nickname: string
  placeholder: string
  submit: string
  variant: 'primary' | 'accent'
  hint?: string
  autoFocus?: boolean
  onSubmit: (body: string) => Promise<void>
}) {
  const [text, setText] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function send() {
    if (busy || !text.trim()) return
    setBusy(true)
    setError(null)
    try {
      await onSubmit(text)
      setText('')
    } catch (e) {
      setError(problem(e))
    } finally {
      setBusy(false)
    }
  }

  const field =
    'rounded-md border border-border bg-surface text-sm text-fg placeholder:text-fg-3 transition duration-200 ' +
    'ease-out focus:border-action focus:shadow-focus focus:outline-none'

  return (
    <form
      className="border-t border-border bg-sunken px-5 py-4"
      onSubmit={(e: FormEvent) => {
        e.preventDefault()
        send()
      }}
    >
      <textarea
        aria-label="Message"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) send()
        }}
        maxLength={500}
        rows={3}
        placeholder={placeholder}
        autoFocus={autoFocus}
        className={`block w-full resize-none px-3 py-2 ${field}`}
      />
      <div className="mt-2 flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-[12px] text-fg-3">
          as <span className="font-mono text-fg-2">{nickname}</span>
        </span>
        <Button type="submit" variant={variant} disabled={busy || !text.trim()}>
          {busy ? 'Posting…' : submit}
        </Button>
      </div>
      {hint && <p className="mt-2 text-[12px] text-fg-3">{hint}</p>}
      {error && (
        <p role="alert" className="mt-2 rounded-sm bg-danger-soft px-3 py-2 text-[12px] text-danger-strong">
          {error}
        </p>
      )}
    </form>
  )
}
