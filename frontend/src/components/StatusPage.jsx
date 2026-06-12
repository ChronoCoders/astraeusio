import { useState, useEffect, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import Navbar from './Navbar'
import Footer from './Footer'

const REFRESH_INTERVAL = 30_000

function statusMeta(status) {
  switch (status) {
    case 'operational': return { dot: 'bg-green-500',  text: 'text-green-400',  label: 'status.operational' }
    case 'degraded':    return { dot: 'bg-yellow-500', text: 'text-yellow-400', label: 'status.degraded'    }
    case 'unknown':     return { dot: 'bg-zinc-500',   text: 'text-zinc-400',   label: 'status.unknown'     }
    default:            return { dot: 'bg-red-500',    text: 'text-red-400',    label: 'status.outage'      }
  }
}

function overallBanner(status) {
  switch (status) {
    case 'operational': return { bg: 'bg-green-500/10 border-green-500/30',  icon: '●', iconCls: 'text-green-400', key: 'status.allOperational' }
    case 'degraded':    return { bg: 'bg-yellow-500/10 border-yellow-500/30', icon: '●', iconCls: 'text-yellow-400', key: 'status.partialOutage'   }
    default:            return { bg: 'bg-red-500/10 border-red-500/30',       icon: '●', iconCls: 'text-red-400',    key: 'status.majorOutage'     }
  }
}

function fmtAgo(ts) {
  if (!ts) return '-'
  const secs = Math.floor(Date.now() / 1000) - ts
  if (secs < 60)   return 'just now'
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`
  return `${Math.floor(secs / 3600)}h ago`
}

function dayColor(s) {
  switch (s) {
    case 'operational': return 'bg-green-500/80'
    case 'degraded':    return 'bg-yellow-500/80'
    case 'outage':      return 'bg-red-500/80'
    default:            return 'bg-zinc-800'
  }
}

function UptimeStrip({ days, label }) {
  if (!days?.length) return null
  return (
    <div className="flex items-center gap-px mt-2" aria-label={label}>
      {days.map((d, i) => (
        <span
          key={i}
          className={`flex-1 h-7 rounded-[1px] ${dayColor(d.status)}`}
          title={d.uptime_pct != null ? `${d.uptime_pct}% uptime` : 'No data'}
        />
      ))}
    </div>
  )
}

export default function StatusPage({ onSignIn }) {
  const { t } = useTranslation()
  const [data,      setData]      = useState(null)
  const [uptime,    setUptime]    = useState(null)
  const [error,     setError]     = useState(false)
  const [refreshed, setRefreshed] = useState(null)

  const load = useCallback(() => {
    fetch('/api/health')
      .then(r => r.ok ? r.json() : Promise.reject())
      .then(d => { setData(d); setError(false); setRefreshed(new Date()) })
      .catch(() => setError(true))
    fetch('/api/health/uptime')
      .then(r => r.ok ? r.json() : Promise.reject())
      .then(u => setUptime(u))
      .catch(() => { /* leave previous value */ })
  }, [])

  useEffect(() => {
    load()
    const id = setInterval(load, REFRESH_INTERVAL)
    return () => clearInterval(id)
  }, [load])

  const c    = data?.components ?? {}
  const u    = uptime?.components ?? {}
  const banner = overallBanner(error ? 'outage' : data?.status)

  const COMPONENTS = [
    { nameKey: 'status.backendApi',  key: 'backend_api', status: error ? 'outage' : (c.backend_api?.status  ?? 'unknown'), lastUpdate: c.backend_api?.last_checked  },
    { nameKey: 'status.mlForecast',  key: 'ml_forecast', status: error ? 'unknown': (c.ml_forecast?.status  ?? 'unknown'), lastUpdate: c.ml_forecast?.last_checked  },
    { nameKey: 'status.database',    key: 'database',    status: error ? 'unknown': (c.database?.status     ?? 'unknown'), lastUpdate: c.database?.last_write       },
    { nameKey: 'status.noaa',        key: 'noaa',        status: error ? 'unknown': (c.noaa?.status         ?? 'unknown'), lastUpdate: c.noaa?.last_update          },
    { nameKey: 'status.nasa',        key: 'nasa',        status: error ? 'unknown': (c.nasa?.status         ?? 'unknown'), lastUpdate: c.nasa?.last_update          },
    { nameKey: 'status.celestrak',   key: 'celestrak',   status: error ? 'unknown': (c.celestrak?.status    ?? 'unknown'), lastUpdate: c.celestrak?.last_update     },
  ]

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">
      <Navbar onSignIn={onSignIn} />

      {/* Hero */}
      <section className="max-w-3xl mx-auto px-6 pt-36 pb-16">
        <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase mb-4">
          {t('status.eyebrow')}
        </p>
        <h1 className="text-4xl md:text-5xl font-thin tracking-tight text-zinc-100">
          {t('status.title')}
        </h1>
      </section>

      {/* Overall banner */}
      <section className="max-w-3xl mx-auto px-6 pb-10">
        <div className={`flex items-center gap-3 border rounded-lg px-5 py-4 ${banner.bg}`}>
          <span className={`text-lg ${banner.iconCls}`}>{banner.icon}</span>
          <span className="text-sm font-medium text-zinc-100">{t(banner.key)}</span>
          {refreshed && (
            <span className="ml-auto text-zinc-500 text-xs font-mono shrink-0">
              {t('status.updatedAt')} {refreshed.toLocaleTimeString()}
            </span>
          )}
        </div>
      </section>

      {/* Component table */}
      <section className="max-w-3xl mx-auto px-6 pb-24">
        <div className="border border-zinc-800 rounded-xl overflow-hidden">
          <div className="px-5 py-3 border-b border-zinc-800 bg-zinc-900/50 flex items-center justify-between gap-3">
            <p className="text-xs font-mono text-zinc-500 uppercase tracking-widest">
              {t('status.components')}
            </p>
            <p className="text-xs font-mono text-zinc-600">
              {t('status.uptimeWindow')}
            </p>
          </div>
          <div className="divide-y divide-zinc-800/60">
            {COMPONENTS.map(comp => {
              const meta = statusMeta(comp.status)
              const up = u[comp.key]
              return (
                <div key={comp.nameKey} className="px-5 py-4 flex flex-col gap-2">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-4">
                      <span className={`w-2 h-2 rounded-full shrink-0 ${meta.dot}`} />
                      <p className="text-sm text-zinc-200">{t(comp.nameKey)}</p>
                    </div>
                    <div className="flex items-center shrink-0 ml-4 gap-4 sm:gap-6">
                      <span className="text-zinc-400 text-xs font-mono w-16 text-right tabular-nums">
                        {up?.uptime_pct != null ? `${up.uptime_pct.toFixed(2)}%` : ''}
                      </span>
                      <span className="text-zinc-600 text-xs font-mono w-16 text-right tabular-nums hidden sm:block">
                        {comp.lastUpdate != null ? fmtAgo(comp.lastUpdate) : ''}
                      </span>
                      <span className={`text-xs font-mono w-24 text-right ${meta.text}`}>{t(meta.label)}</span>
                    </div>
                  </div>
                  <UptimeStrip days={up?.days} label={`${t(comp.nameKey)} 90-day uptime`} />
                </div>
              )
            })}
          </div>
        </div>

        <p className="text-zinc-600 text-xs font-mono mt-4 text-center">
          {t('status.autoRefresh')}
        </p>
      </section>

      {/* Incident history */}
      <section className="max-w-3xl mx-auto px-6 pb-24">
        <div className="border border-zinc-800 rounded-xl overflow-hidden">
          <div className="px-5 py-3 border-b border-zinc-800 bg-zinc-900/50 flex items-center justify-between gap-3">
            <p className="text-xs font-mono text-zinc-500 uppercase tracking-widest">
              {t('status.incidents')}
            </p>
            <div className="flex items-center gap-4">
              <a
                href="/status/feed.xml"
                className="text-xs font-mono text-zinc-500 hover:text-zinc-200 transition-colors flex items-center gap-1.5"
                title={t('status.subscribeTitle')}
              >
                <svg width="10" height="10" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <circle cx="5" cy="19" r="2.5" />
                  <path d="M3 11.5C8.5 11.5 12.5 15.5 12.5 21H15c0-6.6-5.4-12-12-12v2.5z" />
                  <path d="M3 4.5C12.6 4.5 19.5 11.4 19.5 21H22c0-10.5-8.5-19-19-19v2.5z" />
                </svg>
                {t('status.subscribe')}
              </a>
              <p className="text-xs font-mono text-zinc-600">{t('status.incidentsWindow')}</p>
            </div>
          </div>
          <div className="px-5 py-10 flex flex-col items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-green-500" />
            <p className="text-sm text-zinc-300">{t('status.noIncidents')}</p>
            <p className="text-xs text-zinc-600">{t('status.noIncidentsSub')}</p>
          </div>
        </div>
      </section>

      <Footer />
    </div>
  )
}
