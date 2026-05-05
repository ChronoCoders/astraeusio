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
  if (!ts) return '—'
  const secs = Math.floor(Date.now() / 1000) - ts
  if (secs < 60)   return `${secs}s ago`
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`
  return `${Math.floor(secs / 3600)}h ago`
}

function ComponentRow({ nameKey, descKey, status, lastUpdate }) {
  const { t } = useTranslation()
  const meta = statusMeta(status)
  return (
    <div className="flex items-center justify-between py-4 border-b border-zinc-800/60 last:border-0">
      <div className="flex items-center gap-4">
        <span className={`w-2 h-2 rounded-full shrink-0 ${meta.dot}`} />
        <div>
          <p className="text-sm text-zinc-200">{t(nameKey)}</p>
          <p className="text-xs text-zinc-500 mt-0.5">{t(descKey)}</p>
        </div>
      </div>
      <div className="flex items-center gap-6 shrink-0 ml-4">
        {lastUpdate != null && (
          <span className="text-zinc-600 text-xs font-mono hidden sm:block">{fmtAgo(lastUpdate)}</span>
        )}
        <span className={`text-xs font-mono ${meta.text}`}>{t(meta.label)}</span>
      </div>
    </div>
  )
}

export default function StatusPage({ onSignIn }) {
  const { t } = useTranslation()
  const [data,      setData]      = useState(null)
  const [error,     setError]     = useState(false)
  const [refreshed, setRefreshed] = useState(null)

  const load = useCallback(() => {
    fetch('/api/health')
      .then(r => r.ok ? r.json() : Promise.reject())
      .then(d => { setData(d); setError(false); setRefreshed(new Date()) })
      .catch(() => setError(true))
  }, [])

  useEffect(() => {
    load()
    const id = setInterval(load, REFRESH_INTERVAL)
    return () => clearInterval(id)
  }, [load])

  const c    = data?.components ?? {}
  const banner = overallBanner(error ? 'outage' : data?.status)

  const COMPONENTS = [
    { nameKey: 'status.backendApi',  descKey: 'status.backendApiDesc',  status: error ? 'outage' : (c.backend_api?.status  ?? 'unknown'), lastUpdate: c.backend_api?.last_checked  },
    { nameKey: 'status.mlForecast',  descKey: 'status.mlForecastDesc',  status: error ? 'unknown': (c.ml_forecast?.status  ?? 'unknown'), lastUpdate: c.ml_forecast?.last_checked  },
    { nameKey: 'status.database',    descKey: 'status.databaseDesc',    status: error ? 'unknown': (c.database?.status     ?? 'unknown'), lastUpdate: c.database?.last_write       },
    { nameKey: 'status.noaa',        descKey: 'status.noaaDesc',        status: error ? 'unknown': (c.noaa?.status         ?? 'unknown'), lastUpdate: c.noaa?.last_update          },
    { nameKey: 'status.nasa',        descKey: 'status.nasaDesc',        status: error ? 'unknown': (c.nasa?.status         ?? 'unknown'), lastUpdate: c.nasa?.last_update          },
    { nameKey: 'status.celestrak',   descKey: 'status.celestrakDesc',   status: error ? 'unknown': (c.celestrak?.status    ?? 'unknown'), lastUpdate: c.celestrak?.last_update     },
  ]

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">
      <Navbar onSignIn={onSignIn} />

      {/* Hero */}
      <section className="max-w-3xl mx-auto px-6 pt-36 pb-12">
        <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase mb-6">
          {t('status.eyebrow')}
        </p>
        <h1 className="text-4xl font-thin tracking-tight text-zinc-100">
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
          <div className="px-5 py-3 border-b border-zinc-800 bg-zinc-900/50">
            <p className="text-xs font-mono text-zinc-500 uppercase tracking-widest">
              {t('status.components')}
            </p>
          </div>
          <div className="px-5">
            {COMPONENTS.map(c => (
              <ComponentRow key={c.nameKey} {...c} />
            ))}
          </div>
        </div>

        <p className="text-zinc-600 text-xs font-mono mt-4 text-center">
          {t('status.autoRefresh')}
        </p>
      </section>

      <Footer />
    </div>
  )
}
