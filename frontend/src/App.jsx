import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { useApi }          from './lib/useApi'
import { stormInfo, xrayClass, fmtNum } from './lib/utils'
import Sidebar        from './components/Sidebar'
import MetricCard     from './components/MetricCard'
import KpChart        from './components/KpChart'
import ForecastPanel  from './components/ForecastPanel'
import AsteroidTable  from './components/AsteroidTable'
import AlertsList     from './components/AlertsList'
import IssPanel       from './components/IssPanel'
import ApodCard       from './components/ApodCard'
import EpicViewer     from './components/EpicViewer'
import ExoplanetStats from './components/ExoplanetStats'
import AnomalyPanel   from './components/AnomalyPanel'
import SolarWindChart from './components/SolarWindChart'
import XrayFluxChart  from './components/XrayFluxChart'
import KpGauge        from './components/KpGauge'

const FAST = 30_000   // 30 s — live data
const SLOW = 120_000  // 2 min — slower-changing data

function fmtUtc(d) {
  const p = n => String(n).padStart(2, '0')
  return `${d.getUTCFullYear()}-${p(d.getUTCMonth() + 1)}-${p(d.getUTCDate())} ` +
         `${p(d.getUTCHours())}:${p(d.getUTCMinutes())}:${p(d.getUTCSeconds())} UTC`
}

export default function App({ onLogout, onReady }) {
  const { t } = useTranslation()
  const [utcNow, setUtcNow]       = useState(() => fmtUtc(new Date()))
  const [page, setPage]           = useState('dashboard')
  const [sidebarOpen, setSidebar] = useState(false)

  useEffect(() => {
    onReady?.()
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    const id = setInterval(() => setUtcNow(fmtUtc(new Date())), 1000)
    return () => clearInterval(id)
  }, [])

  const kp        = useApi('/api/kp',          FAST)
  const wind      = useApi('/api/solar-wind',  SLOW)
  const xray      = useApi('/api/xray',        SLOW)
  const alerts    = useApi('/api/alerts',      SLOW)
  const iss       = useApi('/api/iss',         FAST)
  const apod      = useApi('/api/apod',        SLOW)
  const epic      = useApi('/api/epic',        SLOW)
  const neo       = useApi('/api/neo',         SLOW)
  const exo       = useApi('/api/exoplanets',  SLOW)
  const forecast  = useApi('/api/kp-forecast', FAST)
  const anomalies = useApi('/api/anomalies',   FAST)

  const latestKp  = kp.data?.at(-1)
  const currentKp = latestKp?.estimated_kp ?? latestKp?.kp_index
  const storm     = stormInfo(currentKp ?? 0)

  const latestWind = wind.data?.find(r => r.proton_speed != null)

  const latestXray = xray.data?.filter(r => r.energy === '0.1-0.8nm')?.at(-1)
  const xClass     = xrayClass(latestXray?.flux)

  return (
    <div className="min-h-screen bg-zinc-950">

      <Sidebar
        page={page}
        onNavigate={setPage}
        open={sidebarOpen}
        onClose={() => setSidebar(false)}
        onLogout={onLogout}
      />

      {/* ── Main content, offset by sidebar width on desktop ───────────── */}
      <div className="lg:ml-[220px] flex flex-col min-h-screen">

        {/* Top bar */}
        <header className="h-14 flex items-center justify-between px-4 border-b border-zinc-800 shrink-0">
          <div className="flex items-center gap-3">
            {/* Hamburger — mobile only */}
            <button
              onClick={() => setSidebar(true)}
              className="lg:hidden text-zinc-400 hover:text-zinc-200 transition-colors p-1"
              aria-label="Open menu"
            >
              <svg width="18" height="18" viewBox="0 0 18 18" fill="none"
                stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
                <line x1="2" y1="4.5" x2="16" y2="4.5" />
                <line x1="2" y1="9"   x2="16" y2="9" />
                <line x1="2" y1="13.5" x2="16" y2="13.5" />
              </svg>
            </button>
            {/* Wordmark — mobile only (desktop brand is in sidebar) */}
            <span className="lg:hidden text-zinc-100 font-thin tracking-[0.2em] text-sm select-none">
              ASTRAEUSIO
            </span>
          </div>
          <span className="text-zinc-500 text-xs font-mono">{utcNow}</span>
        </header>

        {/* Page content */}
        <main className="p-4 flex flex-col gap-3 flex-1">

          {/* ── Dashboard ──────────────────────────────────────────────── */}
          {page === 'dashboard' && <>

            <div className="grid grid-cols-5 gap-3">
              <MetricCard
                label={t('metrics.kpIndex')}
                value={currentKp != null ? fmtNum(currentKp, 2) : null}
                sub={t(storm.key)}
                valueCls={storm.cls}
              />
              <MetricCard
                label={t('metrics.solarWindSpeed')}
                value={latestWind?.proton_speed != null ? fmtNum(latestWind.proton_speed, 0) : null}
                unit="km/s"
                sub={wind.loading ? t('common.loading') : wind.error ? t('common.unavailable') : null}
              />
              <MetricCard
                label={t('metrics.protonDensity')}
                value={latestWind?.proton_density != null ? fmtNum(latestWind.proton_density, 1) : null}
                unit="p/cm³"
                sub={wind.loading ? t('common.loading') : null}
              />
              <MetricCard
                label={t('metrics.xrayClass')}
                value={xClass.label}
                sub={latestXray?.flux != null ? `${latestXray.flux.toExponential(1)} W/m²` : xray.loading ? t('common.loading') : xray.error ? t('common.unavailable') : null}
                valueCls={xClass.cls}
              />
              <MetricCard
                label={t('metrics.stormLevel')}
                value={t(storm.key)}
                sub={`Kp ${currentKp != null ? fmtNum(currentKp, 1) : '—'}`}
                valueCls={storm.cls}
              />
            </div>

            <div className="grid grid-cols-3 gap-3">
              <div className="col-span-2">
                <KpChart records={kp.data} />
              </div>
              <ForecastPanel data={forecast.data} loading={forecast.loading} error={forecast.error} />
            </div>

            <div className="flex flex-col gap-3">
              <AsteroidTable data={neo.data} />
              <div className="grid grid-cols-2 gap-3">
                <AnomalyPanel data={anomalies.data} loading={anomalies.loading} error={anomalies.error} />
                <AlertsList data={alerts.data} />
              </div>
            </div>

            <div className="grid grid-cols-2 gap-3">
              <IssPanel data={iss.data} />
              <ApodCard data={apod.data} />
            </div>

            <div className="grid grid-cols-2 gap-3">
              <EpicViewer data={epic.data} />
              <ExoplanetStats data={exo.data} />
            </div>

          </>}

          {/* ── Charts ─────────────────────────────────────────────────── */}
          {page === 'charts' && (
            <div className="flex flex-col gap-3">
              <div className="grid grid-cols-3 gap-3">
                <div className="col-span-2">
                  <SolarWindChart data={wind.data} />
                </div>
                <KpGauge kp={currentKp} />
              </div>
              <XrayFluxChart data={xray.data} />
            </div>
          )}

          {/* ── Map ────────────────────────────────────────────────────── */}
          {page === 'map' && (
            <div className="flex flex-col gap-3">
              <div className="flex items-center justify-between">
                <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('map.title')}</span>
                <span className="text-zinc-600 text-xs">{t('map.updated')}</span>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-2">
                  <span className="text-zinc-400 text-xs font-mono">{t('map.north')}</span>
                  <img
                    src={`https://services.swpc.noaa.gov/images/aurora-forecast-northern-hemisphere.jpg?v=${Math.floor(Date.now() / 1_800_000)}`}
                    alt="Northern hemisphere auroral oval forecast"
                    className="w-full rounded"
                  />
                </div>
                <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-2">
                  <span className="text-zinc-400 text-xs font-mono">{t('map.south')}</span>
                  <img
                    src={`https://services.swpc.noaa.gov/images/aurora-forecast-southern-hemisphere.jpg?v=${Math.floor(Date.now() / 1_800_000)}`}
                    alt="Southern hemisphere auroral oval forecast"
                    className="w-full rounded"
                  />
                </div>
              </div>
              <p className="text-zinc-600 text-xs text-right">{t('map.credit')}</p>
            </div>
          )}

          {/* ── Alerts ─────────────────────────────────────────────────── */}
          {page === 'alerts' && (
            <div className="flex flex-col gap-3">
              <AnomalyPanel data={anomalies.data} loading={anomalies.loading} error={anomalies.error} />
              <AlertsList data={alerts.data} />
            </div>
          )}

        </main>
      </div>
    </div>
  )
}
