const MINUTE = 60_000
const DAY = 24 * 60 * MINUTE
const EARTH_RADIUS_KM = 6371

type LatLng = { lat: number; lng: number }

/** Great-circle distance in km (haversine), the same sum the server does. */
export function distanceKm(a: LatLng, b: LatLng): number {
  const rad = Math.PI / 180
  const halfDLat = ((b.lat - a.lat) * rad) / 2
  const halfDLng = ((b.lng - a.lng) * rad) / 2
  const h = Math.sin(halfDLat) ** 2 + Math.cos(a.lat * rad) * Math.cos(b.lat * rad) * Math.sin(halfDLng) ** 2
  return 2 * EARTH_RADIUS_KM * Math.asin(Math.sqrt(h))
}

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
