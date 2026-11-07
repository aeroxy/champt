import { type ButtonHTMLAttributes, type ReactNode, useState } from 'react'

type Variant = 'primary' | 'accent' | 'secondary' | 'ghost'

// Ink is the default affirmative action. `accent` is for the one orange
// call to action per view: a second one makes both meaningless.
const variants: Record<Variant, string> = {
  primary: 'bg-ink text-white shadow-sm hover:bg-ink-700',
  accent: 'bg-action text-white shadow-sm hover:bg-action-700',
  secondary: 'border border-border bg-surface text-fg-2 hover:border-border-strong hover:text-fg',
  ghost: 'text-fg-2 hover:bg-muted hover:text-fg',
}

/** `square` is for an icon alone: a 40px square instead of padded text. */
export function Button({
  variant = 'primary',
  square = false,
  className = '',
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; square?: boolean }) {
  return (
    <button
      className={
        'inline-flex h-10 shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-md ' +
        'text-sm font-semibold transition duration-200 ease-out focus-visible:shadow-focus ' +
        'focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50 ' +
        `${square ? 'w-10' : 'px-4'} ${variants[variant]} ${className}`
      }
      {...rest}
    />
  )
}

/** A label for something true: blue, with a live dot. */
export function LiveChip({ children }: { children: ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full border border-status-300 bg-status-soft px-2.5 py-0.5 text-[11px] font-medium text-status-700 tabular">
      <span className="live-dot" />
      {children}
    </span>
  )
}

/** A neutral label, with a plain dot for something that is off. */
export function Chip({ children }: { children: ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full border border-border px-2.5 py-0.5 text-[11px] font-medium text-fg-2">
      <span className="size-1.5 rounded-full bg-border-strong" />
      {children}
    </span>
  )
}

/**
 * Someone's Gravatar, through the link Champt serves it at. Nothing without a
 * link, and `missing` when the link has no picture behind it.
 */
export function Avatar({
  src,
  className = '',
  missing = null,
}: {
  src: string | null
  className?: string
  missing?: ReactNode
}) {
  // The link that failed, so a new link gets a fresh try.
  const [failed, setFailed] = useState<string | null>(null)
  if (!src) return null
  if (failed === src) return <>{missing}</>
  return (
    <img
      src={src}
      alt=""
      onError={() => setFailed(src)}
      className={`shrink-0 rounded-full bg-muted object-cover ${className}`}
    />
  )
}

/** A quiet text-only button, for small actions inside a line of text. */
export function TextButton({ className = '', ...rest }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      className={
        'shrink-0 rounded-xs font-medium text-fg-2 hover:text-fg hover:underline ' +
        `focus-visible:shadow-focus focus-visible:outline-none ${className}`
      }
      {...rest}
    />
  )
}
