const MINUTE = 60_000
const DAY = 24 * 60 * MINUTE

/** "23h 04m left", "12m left", "<1m left". */
export function timeLeft(expiresAt: string, now: number): string {
  const ms = Date.parse(expiresAt) - now
  if (ms <= 0) return 'Expired'
  const minutes = Math.floor(ms / MINUTE)
  if (minutes < 1) return '<1m left'
  const hours = Math.floor(minutes / 60)
  if (hours === 0) return `${minutes}m left`
  return `${hours}h ${String(minutes % 60).padStart(2, '0')}m left`
}

/** How much of a thread's 24 hours is still to run, from 0 to 1. */
export function life(expiresAt: string, now: number): number {
  return Math.min(1, Math.max(0, (Date.parse(expiresAt) - now) / DAY))
}

/** "now", "5m ago", "3h ago", "2d ago". */
export function ago(createdAt: string, now: number): string {
  const minutes = Math.floor((now - Date.parse(createdAt)) / MINUTE)
  if (minutes < 1) return 'now'
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 48) return `${hours}h ago`
  return `${Math.floor(hours / 24)}d ago`
}
