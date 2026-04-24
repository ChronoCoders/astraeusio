import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import * as satellite from 'satellite.js'

const W = 900
const H = 450

function lonLatToXY(lon, lat) {
  return [
    ((lon + 180) / 360) * W,
    ((90 - lat) / 180) * H,
  ]
}

function drawBackground(ctx) {
  ctx.fillStyle = '#050810'
  ctx.fillRect(0, 0, W, H)

  // Lat/lon grid every 30°
  ctx.strokeStyle = 'rgba(148,163,184,0.06)'
  ctx.lineWidth = 0.5
  for (let lon = -180; lon <= 180; lon += 30) {
    const [x] = lonLatToXY(lon, 0)
    ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, H); ctx.stroke()
  }
  for (let lat = -90; lat <= 90; lat += 30) {
    const [, y] = lonLatToXY(0, lat)
    ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(W, y); ctx.stroke()
  }

  // Equator brighter
  ctx.strokeStyle = 'rgba(148,163,184,0.18)'
  ctx.lineWidth = 0.5
  const [, ey] = lonLatToXY(0, 0)
  ctx.beginPath(); ctx.moveTo(0, ey); ctx.lineTo(W, ey); ctx.stroke()

  // Prime meridian brighter
  const [px] = lonLatToXY(0, 0)
  ctx.beginPath(); ctx.moveTo(px, 0); ctx.lineTo(px, H); ctx.stroke()

  // Axis labels
  ctx.fillStyle = 'rgba(148,163,184,0.25)'
  ctx.font = '9px monospace'
  ctx.textAlign = 'center'
  for (let lat = -60; lat <= 60; lat += 30) {
    if (lat === 0) continue
    const [, y] = lonLatToXY(0, lat)
    ctx.fillText(`${lat > 0 ? '+' : ''}${lat}°`, 18, y + 3)
  }
  ctx.textAlign = 'left'
  for (let lon = -150; lon <= 150; lon += 30) {
    if (lon === 0) continue
    const [x] = lonLatToXY(lon, 0)
    ctx.fillText(lon > 0 ? `+${lon}` : `${lon}`, x + 2, H - 3)
  }

  // Border
  ctx.strokeStyle = 'rgba(148,163,184,0.12)'
  ctx.lineWidth = 1
  ctx.strokeRect(0.5, 0.5, W - 1, H - 1)
}

export default function StarlinkMap({ data }) {
  const { t } = useTranslation()
  const canvasRef = useRef(null)
  const [count, setCount] = useState(0)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const ctx = canvas.getContext('2d')

    // Parse all TLE strings once; propagation happens inside draw()
    const satrecs = []
    if (data?.length) {
      for (const sat of data) {
        try {
          satrecs.push(satellite.twoline2satrec(sat.tle_line1, sat.tle_line2))
        } catch {
          // skip malformed TLE
        }
      }
    }

    // Draw empty grid immediately so the canvas isn't blank
    drawBackground(ctx)

    if (!satrecs.length) return

    function draw() {
      const now = new Date()
      ctx.clearRect(0, 0, W, H)
      drawBackground(ctx)

      const gmst = satellite.gstime(now)
      const positions = []

      for (const satrec of satrecs) {
        try {
          const pv = satellite.propagate(satrec, now)
          if (!pv?.position) continue
          const geo = satellite.eciToGeodetic(pv.position, gmst)
          const lat = satellite.degreesLat(geo.latitude)
          const lon = satellite.degreesLong(geo.longitude)
          if (!isFinite(lat) || !isFinite(lon)) continue
          positions.push(lonLatToXY(lon, lat))
        } catch {
          // skip propagation errors
        }
      }

      // Batch all circles into one fill call for performance
      ctx.fillStyle = '#22d3ee'
      ctx.globalAlpha = 0.82
      ctx.beginPath()
      for (const [x, y] of positions) {
        ctx.moveTo(x + 1.5, y)
        ctx.arc(x, y, 1.5, 0, Math.PI * 2)
      }
      ctx.fill()
      ctx.globalAlpha = 1

      setCount(positions.length)
    }

    draw()
    const id = setInterval(draw, 10_000)
    return () => clearInterval(id)
  }, [data])

  const loading = data === null || data === undefined
  const empty   = Array.isArray(data) && data.length === 0

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <span className="text-zinc-400 text-xs font-mono uppercase tracking-widest">
          {t('starlink.title')}
        </span>
        <span className="text-cyan-400 text-xs font-mono">
          {count > 0
            ? t('starlink.count', { count: count.toLocaleString() })
            : loading
              ? t('common.loading')
              : t('starlink.noData')}
        </span>
      </div>

      <div className="relative">
        <canvas
          ref={canvasRef}
          width={W}
          height={H}
          className="w-full rounded"
        />
        {(loading || empty) && (
          <div className="absolute inset-0 flex items-center justify-center">
            <span className="text-zinc-600 text-xs font-mono">
              {loading ? t('common.loading') : t('starlink.noData')}
            </span>
          </div>
        )}
      </div>

      <p className="text-zinc-600 text-xs">{t('starlink.footer')}</p>
    </div>
  )
}
