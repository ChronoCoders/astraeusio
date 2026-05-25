import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { authedFetch } from '../lib/useApi'

const RANGES = ['24h', '7d', '30d']
const TYPES = ['', 'kp_storm', 'solar_wind_speed', 'xray_flare', 'asteroid_close', 'ml_forecast_storm']
const SEVERITIES = ['', 'warning', 'critical']
const PAGE_SIZE = 25

function fmtTs(ts) {
  const d = new Date(ts * 1000)
  const p = n => String(n).padStart(2, '0')
  return `${d.getUTCFullYear()}-${p(d.getUTCMonth() + 1)}-${p(d.getUTCDate())} ${p(d.getUTCHours())}:${p(d.getUTCMinutes())} UTC`
}

function severityCls(s) {
  if (s === 'critical') return 'text-red-400 border-red-500/30 bg-red-500/10'
  if (s === 'warning')  return 'text-yellow-400 border-yellow-500/30 bg-yellow-500/10'
  return 'text-zinc-400 border-zinc-700 bg-zinc-800/50'
}

function typeIcon(type) {
  if (type === 'kp_storm' || type === 'ml_forecast_storm') return '⚡'
  if (type === 'xray_flare') return '☀'
  if (type === 'solar_wind_speed') return '💨'
  if (type === 'asteroid_close') return '☄'
  return '●'
}

export default function EventsPage() {
  const { t } = useTranslation()
  const [range, setRange]       = useState('7d')
  const [type, setType]         = useState('')
  const [severity, setSeverity] = useState('')
  const [page, setPage]         = useState(1)
  const [data, setData]         = useState(null)
  const [loading, setLoading]   = useState(false)
  const [error, setError]       = useState(null)

  // Filter changes reset to page 1. Done in the handlers (not an effect) to
  // avoid an extra render pass.
  const changeRange    = (r) => { setRange(r); setPage(1) }
  const changeType     = (v) => { setType(v); setPage(1) }
  const changeSeverity = (v) => { setSeverity(v); setPage(1) }

  useEffect(() => {
    let cancelled = false
    const ctrl = new AbortController()
    async function run() {
      setLoading(true)
      setError(null)
      const params = new URLSearchParams({
        range,
        page: String(page),
        page_size: String(PAGE_SIZE),
      })
      if (type)     params.set('type', type)
      if (severity) params.set('severity', severity)
      try {
        const res = await authedFetch(`/api/events?${params.toString()}`, { signal: ctrl.signal })
        if (!res.ok) throw new Error(`HTTP ${res.status}`)
        const json = await res.json()
        if (!cancelled) { setData(json); setError(null) }
      } catch (e) {
        if (!cancelled && e.name !== 'AbortError') setError(e.message || 'failed to load')
      } finally {
        if (!cancelled) setLoading(false)
      }
    }
    run()
    return () => { cancelled = true; ctrl.abort() }
  }, [range, type, severity, page])

  const total = data?.total ?? 0
  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE))
  const events = data?.events ?? []

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('eventsPage.title')}</span>
        <div className="flex items-center gap-1 bg-zinc-900 border border-zinc-800 rounded p-0.5">
          {RANGES.map(r => (
            <button
              key={r}
              onClick={() => changeRange(r)}
              className={`px-3 py-1 text-xs font-mono rounded transition-colors ${
                range === r ? 'bg-zinc-700 text-zinc-100' : 'text-zinc-500 hover:text-zinc-300'
              }`}
            >
              {r}
            </button>
          ))}
        </div>
      </div>

      <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-wrap items-center gap-3">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('eventsPage.filters')}</span>
        <select
          name="event-type"
          value={type}
          onChange={e => changeType(e.target.value)}
          className="bg-zinc-950 border border-zinc-800 rounded px-3 py-1.5 text-sm text-zinc-200 focus:outline-none focus:border-zinc-600"
        >
          {TYPES.map(ty => (
            <option key={ty || 'all-types'} value={ty}>
              {ty ? t(`eventsPage.types.${ty}`) : t('eventsPage.allTypes')}
            </option>
          ))}
        </select>
        <select
          name="event-severity"
          value={severity}
          onChange={e => changeSeverity(e.target.value)}
          className="bg-zinc-950 border border-zinc-800 rounded px-3 py-1.5 text-sm text-zinc-200 focus:outline-none focus:border-zinc-600"
        >
          {SEVERITIES.map(sv => (
            <option key={sv || 'all-sev'} value={sv}>
              {sv ? t(`eventsPage.severity.${sv}`) : t('eventsPage.allSeverity')}
            </option>
          ))}
        </select>
        <div className="ml-auto text-zinc-500 text-xs font-mono">
          {loading ? '…' : t('eventsPage.found', { count: total })}
        </div>
      </div>

      <div className="bg-zinc-900 border border-zinc-800 rounded overflow-hidden">
        {error && (
          <div className="px-5 py-6 text-red-400 text-sm">{error}</div>
        )}
        {!error && events.length === 0 && !loading && (
          <div className="px-5 py-10 text-center text-zinc-500 text-sm">{t('eventsPage.empty')}</div>
        )}
        {events.length > 0 && (
          <div className="divide-y divide-zinc-800">
            {events.map((ev, i) => (
              <div key={`${ev.type}-${ev.source_ref}-${i}`} className="px-5 py-4 flex items-start gap-4">
                <span className="text-xl shrink-0 leading-none mt-0.5">{typeIcon(ev.type)}</span>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-3 flex-wrap">
                    <span className="text-sm text-zinc-100 font-medium">
                      {t(`eventsPage.types.${ev.type}`, ev.type)}
                    </span>
                    <span className={`text-xs font-mono px-2 py-0.5 rounded border ${severityCls(ev.severity)}`}>
                      {t(`eventsPage.severity.${ev.severity}`, ev.severity)}
                    </span>
                    <span className="text-zinc-600 text-xs font-mono">{fmtTs(ev.detected_at)}</span>
                  </div>
                  <p className="text-zinc-400 text-sm mt-1.5 leading-relaxed break-words">{ev.message}</p>
                  <p className="text-zinc-700 text-xs font-mono mt-1.5">{ev.source_ref}</p>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {totalPages > 1 && (
        <div className="flex items-center justify-between">
          <span className="text-zinc-500 text-xs font-mono">
            {t('eventsPage.page', { page, total: totalPages })}
          </span>
          <div className="flex items-center gap-2">
            <button
              onClick={() => setPage(p => Math.max(1, p - 1))}
              disabled={page <= 1 || loading}
              className="px-3 py-1.5 text-xs font-mono rounded border border-zinc-700 text-zinc-300 hover:border-zinc-500 hover:text-zinc-100 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            >
              {t('eventsPage.prev')}
            </button>
            <button
              onClick={() => setPage(p => Math.min(totalPages, p + 1))}
              disabled={page >= totalPages || loading}
              className="px-3 py-1.5 text-xs font-mono rounded border border-zinc-700 text-zinc-300 hover:border-zinc-500 hover:text-zinc-100 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            >
              {t('eventsPage.next')}
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
