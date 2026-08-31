import { useState, useEffect, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import Navbar from './Navbar'
import Footer from './Footer'

const REFRESH_INTERVAL = 30_000
// Matches DAYS in the uptime handler.
const UPTIME_WINDOW_DAYS = 90

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

// Days of history behind the percentage, or null when it covers the whole
// window and needs no qualifier.
function partialWindow(up) {
  const recorded = up?.recorded_days
  if (recorded == null || recorded <= 0) return null
  if (recorded >= UPTIME_WINDOW_DAYS) return null
  return recorded
}

function dayColor(s) {
  switch (s) {
    case 'operational': return 'bg-green-500/80'
    case 'degraded':    return 'bg-yellow-500/80'
    case 'outage':      return 'bg-red-500/80'
    default:            return 'bg-zinc-800'
  }
}

function UptimeStrip({ days, label, noDataLabel }) {
  if (!days?.length) return null
  return (
    <div className="flex items-center gap-px mt-2" aria-label={label}>
      {days.map((d, i) => (
        <span
          key={i}
          className={[
            'flex-1 h-7 rounded-[1px]',
            dayColor(d.status),
            // A day nobody recorded is drawn hollow, so it reads as absent
            // rather than as a filled bar in a dark colour.
            d.status === 'no_data' ? 'border border-zinc-800 bg-transparent' : '',
          ].join(' ')}
          title={d.uptime_pct != null ? `${d.uptime_pct}% uptime` : noDataLabel}
        />
      ))}
    </div>
  )
}

// Display order for the components we already know about.
//
// A hint laid over the payload, not the source of what gets rendered.
// /api/health decides which components exist; this decides where the familiar
// ones sit. Anything the backend publishes and this array does not name is
// appended rather than dropped.
//
// It used to be the source: a literal list of rows, so a component the backend
// published and the list omitted was invisible on the page whose whole job is
// to make things visible. That is the third time this shape has cost something,
// after the poller/anomaly mapping and the interval boot line, and it is the
// only one of the three a user could see.
const ORDER = [
  'backend_api',
  'ml_forecast',
  'database',
  // Each space weather series reports on its own. One of them stopped for
  // forty days while a shared NOAA row stayed green.
  'noaa_kp',
  'noaa_kp_3h',
  'noaa_solar_wind',
  'noaa_xray',
  'noaa_imf',
  'noaa_dst',
  // Episodic, so this one reports whether the poll is returning a live feed
  // rather than how old the newest alert is.
  'noaa_alerts',
  'iss',
  // Likewise for NASA. A single row here averaged apod, neo and epic, so the
  // daily APOD kept it green with the other two dead.
  'nasa_apod',
  'nasa_neo',
  'nasa_epic',
  'nasa_exoplanets',
  'celestrak',
]

// Words the humanised fallback should shout rather than sentence-case.
const ACRONYMS = new Set(['noaa', 'nasa', 'iss', 'ml', 'api', 'imf', 'dst', 'neo', 'epic', 'apod', 'kp', 'tle', 'db'])

// The label for a component with no translation, which is the case a hardcoded
// array exists to avoid. It is never blank and never a dropped row: the key
// renders as itself, tidied, so a component published before its locale strings
// land reads as "NOAA Alerts" rather than vanishing.
function humanise(key) {
  return key
    .split('_')
    .map(w => (ACRONYMS.has(w) ? w.toUpperCase() : w.charAt(0).toUpperCase() + w.slice(1)))
    .join(' ')
}

// `noaa_kp_3h` is spelled `status.noaaKp3h` in the locale files. Derived rather
// than mapped, so a new component needs a locale key in en.json and tr.json and
// no wiring here at all.
function labelKey(key) {
  return 'status.' + key.replace(/_(.)/g, (_, ch) => ch.toUpperCase())
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

  // Rendered from what /api/health actually published. When the endpoint is
  // unreachable there is no payload to enumerate, so ORDER stands in as the
  // skeleton, because a blank page is the worst answer at exactly the moment
  // somebody is looking at this one.
  const keys = Object.keys(c).length > 0
    ? [...ORDER.filter(k => k in c), ...Object.keys(c).filter(k => !ORDER.includes(k)).sort()]
    : ORDER

  const COMPONENTS = keys.map(key => ({
    key,
    name: t(labelKey(key), { defaultValue: humanise(key) }),
    status: error
      ? (key === 'backend_api' ? 'outage' : 'unknown')
      : (c[key]?.status ?? 'unknown'),
    // Which timestamp a component carries depends on what it measures, so take
    // whichever it published rather than knowing per component.
    lastUpdate: c[key]?.last_update ?? c[key]?.last_checked ?? c[key]?.last_write,
  }))

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
                <div key={comp.key} className="px-5 py-4 flex flex-col gap-2">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-4">
                      <span className={`w-2 h-2 rounded-full shrink-0 ${meta.dot}`} />
                      <p className="text-sm text-zinc-200">{comp.name}</p>
                    </div>
                    <div className="flex items-center shrink-0 ml-4 gap-4 sm:gap-6">
                      <span className="text-zinc-400 text-xs font-mono w-16 text-right tabular-nums">
                        {up?.uptime_pct != null ? `${up.uptime_pct.toFixed(2)}%` : ''}
                      </span>
                      {/* The percentage covers only the days actually recorded.
                          Saying so means a component added last week is not read
                          against a 90 day window it was never in. */}
                      <span className="text-zinc-600 text-xs font-mono w-20 text-right hidden sm:block">
                        {partialWindow(up) != null
                          ? t('status.recordedDays', { count: partialWindow(up) })
                          : ''}
                      </span>
                      <span className="text-zinc-600 text-xs font-mono w-16 text-right tabular-nums hidden sm:block">
                        {comp.lastUpdate != null ? fmtAgo(comp.lastUpdate) : ''}
                      </span>
                      <span className={`text-xs font-mono w-24 text-right ${meta.text}`}>{t(meta.label)}</span>
                    </div>
                  </div>
                  <UptimeStrip
                    days={up?.days}
                    label={`${comp.name} ${t('status.uptimeWindow')}`}
                    noDataLabel={t('common.noData')}
                  />
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
