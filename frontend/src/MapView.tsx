import { useEffect, useRef, useState } from 'react'
import {
  type GeoJSONSource,
  type MapLayerMouseEvent,
  MapLibreMap,
  NavigationControl,
  setWorkerUrl,
} from 'maplibre-gl'
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?url'
import type { Pin } from './api'
import { life } from './format'

// MapLibre looks for its worker next to its own module, and bundling moves
// that module (in dev and in the build alike), so the map never draws. The
// worker has no imports of its own: point MapLibre at the copy Vite emits.
setWorkerUrl(workerUrl)

// OpenFreeMap: OpenStreetMap vector tiles, no key and no account.
// Positron is the quiet light basemap, which suits the warm paper.
const STYLE = 'https://tiles.openfreemap.org/styles/positron'

// Map paint can't read CSS variables, so the two accents are spelled out.
const ACTION = '#D85A30' // pins: you can click them
const STATUS = '#2E6BE6' // you: you are here
const INK = '#0B0B0B'

export type Spot = { lat: number; lng: number }

/** Where to fly. A new `key` flies again, even to the same spot. */
export type Focus = { spot: Spot; zoom: number; key: number }

type Props = {
  pins: Pin[]
  now: number
  me: Spot | null
  selected: number | null
  focus: Focus | null
  /** Pins under a click: one, or several stacked on the same spot. */
  onPick: (ids: number[]) => void
  /** The visible area changed: west,south,east,north. */
  onView: (bbox: string) => void
}

const NOTHING: GeoJSON.FeatureCollection = { type: 'FeatureCollection', features: [] }

export function MapView({ pins, now, me, selected, focus, onPick, onView }: Props) {
  const container = useRef<HTMLDivElement>(null)
  const map = useRef<MapLibreMap | null>(null)
  const [ready, setReady] = useState(false)
  // The map's listeners are attached once, so they read the latest
  // callbacks through this ref.
  const handlers = useRef({ onPick, onView })
  handlers.current = { onPick, onView }

  useEffect(() => {
    const m = new MapLibreMap({
      container: container.current!,
      style: STYLE,
      center: [0, 25],
      zoom: 1.3,
      attributionControl: { compact: true },
    })
    m.addControl(new NavigationControl({ showCompass: false }), 'bottom-right')

    m.on('load', () => {
      m.addSource('threads', { type: 'geojson', data: NOTHING })
      m.addSource('me', { type: 'geojson', data: NOTHING })
      m.addLayer({
        id: 'me-halo',
        type: 'circle',
        source: 'me',
        paint: { 'circle-radius': 16, 'circle-color': STATUS, 'circle-opacity': 0.16 },
      })
      m.addLayer({
        id: 'me',
        type: 'circle',
        source: 'me',
        paint: {
          'circle-radius': 6,
          'circle-color': STATUS,
          'circle-stroke-width': 2,
          'circle-stroke-color': '#FFFFFF',
        },
      })
      m.addLayer({
        id: 'pins',
        type: 'circle',
        source: 'threads',
        paint: {
          'circle-radius': ['interpolate', ['linear'], ['zoom'], 2, 5, 14, 8],
          'circle-color': ACTION,
          // A pin fades as its 24 hours run down.
          'circle-opacity': ['interpolate', ['linear'], ['get', 'life'], 0, 0.35, 1, 1],
          'circle-stroke-width': 2,
          'circle-stroke-color': '#FFFFFF',
        },
      })
      m.addLayer({
        id: 'pin-selected',
        type: 'circle',
        source: 'threads',
        filter: ['==', ['get', 'id'], -1],
        paint: {
          'circle-radius': ['interpolate', ['linear'], ['zoom'], 2, 7, 14, 11],
          'circle-color': ACTION,
          'circle-stroke-width': 3,
          'circle-stroke-color': INK,
        },
      })
      setReady(true)
      handlers.current.onView(bboxOf(m))
    })

    m.on('moveend', () => handlers.current.onView(bboxOf(m)))
    m.on('click', 'pins', (event: MapLayerMouseEvent) => {
      const ids = [...new Set((event.features ?? []).map((f) => Number(f.properties?.id)))]
      if (ids.length > 0) handlers.current.onPick(ids)
    })
    m.on('mouseenter', 'pins', () => {
      m.getCanvas().style.cursor = 'pointer'
    })
    m.on('mouseleave', 'pins', () => {
      m.getCanvas().style.cursor = ''
    })

    map.current = m
    return () => {
      m.remove()
      map.current = null
      setReady(false)
    }
  }, [])

  useEffect(() => {
    if (!ready) return
    map.current?.getSource<GeoJSONSource>('threads')?.setData({
      type: 'FeatureCollection',
      features: pins.map((pin) => ({
        type: 'Feature',
        geometry: { type: 'Point', coordinates: [pin.lng, pin.lat] },
        properties: { id: pin.id, life: life(pin.expires_at, now) },
      })),
    })
  }, [ready, pins, now])

  useEffect(() => {
    if (!ready) return
    map.current?.getSource<GeoJSONSource>('me')?.setData(
      me ? { type: 'Point', coordinates: [me.lng, me.lat] } : NOTHING,
    )
  }, [ready, me])

  useEffect(() => {
    if (!ready) return
    map.current?.setFilter('pin-selected', ['==', ['get', 'id'], selected ?? -1])
  }, [ready, selected])

  useEffect(() => {
    if (!focus) return
    map.current?.flyTo({ center: [focus.spot.lng, focus.spot.lat], zoom: focus.zoom, essential: true })
  }, [focus])

  // MapLibre gives its container `position: relative`, so the positioning
  // lives on a wrapper it doesn't touch.
  return (
    <div className="absolute inset-0">
      <div ref={container} className="h-full w-full" />
    </div>
  )
}

function bboxOf(m: MapLibreMap): string {
  const b = m.getBounds()
  return [b.getWest(), b.getSouth(), b.getEast(), b.getNorth()].map((x) => x.toFixed(5)).join(',')
}
