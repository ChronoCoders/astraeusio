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

// Days of history behind the percentage, or null when it covers the whole
// window and needs no qualifier.
function partialWindow(up) {
  const recorded = up?.recorded_days
  if (recorded == null || recorded <= 0) return null
  if (recorded >= UPTIME_WINDOW_DAYS) return null
  return recorded
}

// A group's history, folded from its members' strips.
//
// A day is only as good as its worst member that reported, and a day nobody
// reported is absent rather than an outage. Recorded days counts the days with
// any data at all, so a group is never scored against a window it was not
// observed in, and the percentage is the mean of those days.
function groupUptime(members, u) {
  const strips = members.map(m => u[m]?.days).filter(Boolean)
  if (!strips.length) return null

  const length = Math.max(...strips.map(d => d.length))
  const days = Array.from({ length }, (_, i) => {
    const reported = strips
      .map(d => d[i])
      .filter(d => d && d.status !== 'no_data')
    if (!reported.length) return { status: 'no_data', uptime_pct: null }
    return {
      status: worst(reported.map(d => d.status)),
      uptime_pct: Math.min(...reported.map(d => d.uptime_pct ?? 100)),
    }
  })

  const recorded = days.filter(d => d.status !== 'no_data')
  if (!recorded.length) return { days, uptime_pct: null, recorded_days: 0, incidentDays: 0 }
  return {
    days,
    uptime_pct: recorded.reduce((a, d) => a + d.uptime_pct, 0) / recorded.length,
    recorded_days: recorded.length,
    incidentDays: recorded.filter(d => d.status !== 'operational').length,
  }
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

// What a visitor is actually asking: does the product work.
//
// Sixteen rows could not answer that. Most of them named a data feed nobody
// outside the team has heard of, and one of them going quiet put "Partial
// Outage" at the top of the page, which a satellite operator reads as our space
// weather data being broken. Three rows answer the question; the sixteen
// components are still published by /api/health and still watched by the cron
// checks, which is where per-feed detail belongs.
//
// Membership is declared, and that is the only hardcoded part. Anything
// /api/health publishes that no group claims and HIDDEN does not name lands in
// a group of its own rather than disappearing, so the default for something new
// is still that it shows up. That property was bought once already, when this
// page rendered from a literal list and a component the backend published was
// invisible.
const GROUPS = [
  { key: 'platform', members: ['backend_api', 'database', 'ml_forecast'] },
  {
    key: 'spaceWeather',
    members: [
      'noaa_kp',
      'noaa_kp_3h',
      'noaa_solar_wind',
      'noaa_xray',
      'noaa_imf',
      'noaa_dst',
      // Episodic rather than a time series, so it reports whether the poll is
      // returning a live feed rather than how old the newest alert is.
      'noaa_alerts',
    ],
  },
  { key: 'satellites', members: ['iss', 'celestrak'] },
]

// Monitored, mailed about, and off this page. An astronomy picture failing to
// fetch is not a statement about the product. /api/health still publishes them
// and component-check.sh still alerts on them; the backend excludes the same
// four from its overall status, so the page and the API agree.
const HIDDEN = ['nasa_apod', 'nasa_epic', 'nasa_neo', 'nasa_exoplanets']

// Worst member decides. A group with one dead feed is partly broken and should
// say so: averaging would let a dead feed hide behind five live ones, which is
// how the magnetometer sat dead for forty days.
const RANK = { outage: 3, degraded: 2, unknown: 1, operational: 0 }
function worst(statuses) {
  return statuses.reduce(
    (acc, s) => ((RANK[s] ?? 1) > (RANK[acc] ?? 1) ? s : acc),
    'operational',
  )
}

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

  // Anything published that no group claims and HIDDEN does not name gets a
  // group of its own, so a new component still appears rather than being
  // dropped by a map that has not heard of it.
  const claimed = new Set([...GROUPS.flatMap(g => g.members), ...HIDDEN])
  const unclaimed = Object.keys(c).filter(k => !claimed.has(k)).sort()
  const groupList = unclaimed.length
    ? [...GROUPS, { key: 'other', members: unclaimed }]
    : GROUPS

  const componentName = key => t(labelKey(key), { defaultValue: humanise(key) })

  const ROWS = groupList.map(group => {
    // On an unreachable endpoint there is nothing to enumerate, so the declared
    // membership stands in: a blank page is the worst answer at exactly the
    // moment somebody loads this one.
    const members = group.members.map(key => ({
      key,
      name: componentName(key),
      status: error
        ? (key === 'backend_api' ? 'outage' : 'unknown')
        : (c[key]?.status ?? 'unknown'),
    }))
    const status = worst(members.map(m => m.status))
    // Read from the payload, never a fixed sentence per group, so a member that
    // goes quiet tomorrow is named without anyone editing this file.
    const affected = members.filter(m => m.status !== 'operational')
    const up = groupUptime(group.members, u)
    return { key: group.key, name: t(`status.group.${group.key}`), status, affected, up }
  })

  const overall = error ? 'outage' : worst(ROWS.map(r => r.status))
  const banner = overallBanner(overall)
  const incidentDays = ROWS.reduce((n, r) => Math.max(n, r.up?.incidentDays ?? 0), 0)

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
            {ROWS.map(row => {
              const meta = statusMeta(row.status)
              return (
                <div key={row.key} className="px-5 py-4 flex flex-col gap-2">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-4 min-w-0">
                      <span className={`w-2 h-2 rounded-full shrink-0 ${meta.dot}`} />
                      <p className="text-sm text-zinc-200 truncate">{row.name}</p>
                    </div>
                    <div className="flex items-center shrink-0 ml-4 gap-4 sm:gap-6">
                      <span className="text-zinc-400 text-xs font-mono w-16 text-right tabular-nums">
                        {row.up?.uptime_pct != null ? `${row.up.uptime_pct.toFixed(2)}%` : ''}
                      </span>
                      {/* The percentage covers only the days actually recorded,
                          so a group whose newest member joined last week is not
                          read against a 90 day window it was never in. */}
                      <span className="text-zinc-600 text-xs font-mono w-20 text-right hidden sm:block">
                        {partialWindow(row.up) != null
                          ? t('status.recordedDays', { count: partialWindow(row.up) })
                          : ''}
                      </span>
                      <span className={`text-xs font-mono w-24 text-right ${meta.text}`}>{t(meta.label)}</span>
                    </div>
                  </div>
                  {/* Which members are affected, from the payload. Empty stays
                      empty: nothing is said when everything is fine. */}
                  {row.affected.length > 0 && (
                    <p className="text-zinc-500 text-xs pl-6">
                      {t(`status.affected.${row.status}`, {
                        names: row.affected.map(m => m.name).join(', '),
                      })}
                    </p>
                  )}
                  <UptimeStrip
                    days={row.up?.days}
                    label={`${row.name} ${t('status.uptimeWindow')}`}
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
          {/* Counted from the same 90 day history the strips above draw, so
              this section is a record rather than a claim. It used to say every
              system had been running normally, in fixed text, while a component
              was degraded on the same page. */}
          <div className="px-5 py-10 flex flex-col items-center gap-2">
            <span className={`w-2 h-2 rounded-full ${incidentDays > 0 ? 'bg-yellow-500' : 'bg-green-500'}`} />
            <p className="text-sm text-zinc-300">
              {incidentDays > 0
                ? t('status.incidentDays', { count: incidentDays })
                : t('status.noIncidents')}
            </p>
          </div>
        </div>
      </section>

      <Footer />
    </div>
  )
}
