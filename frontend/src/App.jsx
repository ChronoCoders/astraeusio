import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { useApi }          from './lib/useApi'
import { stormInfo, xrayClass, fmtNum } from './lib/utils'
import MetricCard     from './components/MetricCard'
import KpChart        from './components/KpChart'
import ForecastPanel  from './components/ForecastPanel'
import AsteroidTable  from './components/AsteroidTable'
import AlertsList     from './components/AlertsList'
import IssPanel       from './components/IssPanel'
import ApodCard       from './components/ApodCard'
import EpicViewer     from './components/EpicViewer'
import ExoplanetStats from './components/ExoplanetStats'

const FAST = 30_000   // 30 s — live data
const SLOW = 120_000  // 2 min — slower-changing data

const LANGS = ['en', 'tr']

function fmtUtc(d) {
  const p = n => String(n).padStart(2, '0')
  return `${d.getUTCFullYear()}-${p(d.getUTCMonth() + 1)}-${p(d.getUTCDate())} ` +
         `${p(d.getUTCHours())}:${p(d.getUTCMinutes())}:${p(d.getUTCSeconds())} UTC`
}

export default function App({ onLogout }) {
  const { t, i18n } = useTranslation()
  const [utcNow, setUtcNow] = useState(() => fmtUtc(new Date()))

  useEffect(() => {
    const id = setInterval(() => setUtcNow(fmtUtc(new Date())), 1000)
    return () => clearInterval(id)
  }, [])

  const kp       = useApi('/api/kp',          FAST)
  const wind     = useApi('/api/solar-wind',  SLOW)
  const xray     = useApi('/api/xray',        SLOW)
  const alerts   = useApi('/api/alerts',      SLOW)
  const iss      = useApi('/api/iss',         FAST)
  const apod     = useApi('/api/apod',        SLOW)
  const epic     = useApi('/api/epic',        SLOW)
  const neo      = useApi('/api/neo',         SLOW)
  const exo      = useApi('/api/exoplanets',  SLOW)
  const forecast = useApi('/api/kp-forecast', FAST)

  const latestKp  = kp.data?.at(-1)
  const currentKp = latestKp?.estimated_kp ?? latestKp?.kp_index
  const storm     = stormInfo(currentKp ?? 0)

  const latestWind = wind.data?.find(r => r.proton_speed != null)

  const latestXray = xray.data?.filter(r => r.energy === '0.1-0.8nm')?.at(-1)
  const xClass     = xrayClass(latestXray?.flux)

  return (
    <div className="min-h-screen p-4 max-w-screen-2xl mx-auto flex flex-col gap-3">

      <header className="flex items-center justify-between pb-2 border-b border-zinc-800">
        <h1 className="text-zinc-100 font-semibold tracking-tight">{t('appTitle')}</h1>
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-1">
            {LANGS.map(lng => (
              <button
                key={lng}
                onClick={() => i18n.changeLanguage(lng)}
                className={`text-xs font-mono px-2 py-0.5 rounded transition-colors ${
                  i18n.language === lng
                    ? 'bg-zinc-700 text-zinc-100'
                    : 'text-zinc-500 hover:text-zinc-300'
                }`}
              >
                {lng.toUpperCase()}
              </button>
            ))}
          </div>
          <span className="text-zinc-500 text-xs font-mono">{utcNow}</span>
          <button
            onClick={onLogout}
            className="text-xs font-mono px-2 py-0.5 rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors"
          >
            {t('auth.logout')}
          </button>
        </div>
      </header>

      {/* Row 1 — Metric cards */}
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

      {/* Row 2 — Kp chart + ML forecast */}
      <div className="grid grid-cols-3 gap-3">
        <div className="col-span-2">
          <KpChart records={kp.data} />
        </div>
        <ForecastPanel data={forecast.data} loading={forecast.loading} error={forecast.error} />
      </div>

      {/* Row 3 — Asteroid table + Alerts */}
      <div className="grid grid-cols-5 gap-3">
        <div className="col-span-3">
          <AsteroidTable data={neo.data} />
        </div>
        <div className="col-span-2">
          <AlertsList data={alerts.data} />
        </div>
      </div>

      {/* Row 4 — ISS + APOD */}
      <div className="grid grid-cols-2 gap-3">
        <IssPanel data={iss.data} />
        <ApodCard data={apod.data} />
      </div>

      {/* Row 5 — EPIC + Exoplanets */}
      <div className="grid grid-cols-2 gap-3">
        <EpicViewer data={epic.data} />
        <ExoplanetStats data={exo.data} />
      </div>

    </div>
  )
}
