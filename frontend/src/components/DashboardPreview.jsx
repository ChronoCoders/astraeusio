import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { stormInfo, fmtNum } from '../lib/utils'
import Logo from './Logo'
import MetricCard from './MetricCard'
import KpChart from './KpChart'
import ForecastPanel from './ForecastPanel'

// The mini-dashboard is laid out at this fixed design width, then uniformly
// scaled to fit the frame. The frame height tracks the canvas's natural height
// (× scale) so nothing is clipped.
const DESIGN_W = 1180

// Representative quiet-conditions fallback so the preview is always populated
// (live public data replaces these whenever it's available).
const SAMPLE_KP = (() => {
  const base = Date.now() / 1000 - 24 * 3600
  const vals = [1.0, 1.3, 2.0, 2.3, 1.7, 2.0, 2.7, 3.0, 2.3, 2.0, 1.7, 2.3, 2.7, 3.3, 2.7, 2.0]
  return vals.map((v, i) => ({
    time_tag: new Date((base + i * 5400) * 1000).toISOString().slice(0, 19),
    kp_index: Math.round(v),
    estimated_kp: v,
  }))
})()
const SAMPLE_WIND = { speed: 412, density: 4.8 }
const SAMPLE_FORECAST = { predicted_kp: 2.4, ci_lower: 2.1, ci_upper: 2.7, uncertainty: 0.18 }

const NAV = ['dashboard', 'forecast', 'charts', 'map', 'alerts', 'events', 'reports', 'api', 'settings']

function MiniSidebar({ t }) {
  return (
    <div className="w-[200px] shrink-0 bg-zinc-950 border-r border-zinc-800 flex flex-col">
      <div className="h-14 flex items-center gap-2.5 px-5 border-b border-zinc-800">
        <Logo size={20} className="shrink-0" />
        <span className="text-zinc-100 font-thin tracking-[0.2em] text-sm">ASTRAEUSIO</span>
      </div>
      <nav className="flex flex-col gap-1 p-3">
        {NAV.map((id, i) => (
          <span
            key={id}
            className={`px-3 py-2 text-sm font-mono tracking-wide rounded ${
              i === 0 ? 'bg-zinc-800 text-zinc-100' : 'text-zinc-500'
            }`}
          >
            {t(`nav.${id}`)}
          </span>
        ))}
      </nav>
    </div>
  )
}

function DashboardCanvas({ kpData, wind, forecastData }) {
  const { t } = useTranslation()
  const series = kpData && kpData.length ? kpData : SAMPLE_KP
  const w = wind && wind.speed != null ? wind : SAMPLE_WIND
  const fc = forecastData && forecastData.predicted_kp != null ? forecastData : SAMPLE_FORECAST
  const latestKp = series.filter(r => r.estimated_kp > 0).at(-1)?.estimated_kp ?? 0
  const storm = stormInfo(latestKp)

  return (
    <div className="flex bg-zinc-950 text-left">
      <MiniSidebar t={t} />
      <div className="flex-1 p-4 flex flex-col gap-3 min-w-0">
        <div className="grid grid-cols-5 gap-3">
          <MetricCard label={t('metrics.kpIndex')} value={fmtNum(latestKp, 2)} sub={t(storm.key)} valueCls={storm.cls} />
          <MetricCard label={t('metrics.solarWindSpeed')} value={fmtNum(w.speed, 0)} unit="km/s" />
          <MetricCard label={t('metrics.protonDensity')} value={fmtNum(w.density, 1)} unit="p/cm³" />
          <MetricCard label={t('metrics.xrayClass')} value="B2.4" sub="2.4e-7 W/m²" />
          <MetricCard label={t('metrics.stormLevel')} value={t(storm.key)} sub={`Kp ${fmtNum(latestKp, 1)}`} valueCls={storm.cls} />
        </div>
        <div className="grid grid-cols-3 gap-3">
          <div className="col-span-2">
            <KpChart records={series} />
          </div>
          <ForecastPanel data={fc} loading={false} onNavigate={() => {}} />
        </div>
      </div>
    </div>
  )
}

export default function DashboardPreview({ kpData, wind, forecastData }) {
  const { t } = useTranslation()
  const frameRef = useRef(null)   // scaled viewport — source of available width
  const canvasRef = useRef(null)  // unscaled canvas — source of natural height
  const sectionRef = useRef(null)
  const [scale, setScale] = useState(0.9)
  const [height, setHeight] = useState(420)
  const [visible, setVisible] = useState(false)

  useEffect(() => {
    const frame = frameRef.current
    const canvas = canvasRef.current
    if (!frame || !canvas) return
    const update = () => {
      const s = frame.clientWidth / DESIGN_W
      setScale(s)
      setHeight(canvas.offsetHeight * s)
    }
    update()
    const ro = new ResizeObserver(update)
    ro.observe(frame)
    ro.observe(canvas)
    return () => ro.disconnect()
  }, [])

  useEffect(() => {
    const el = sectionRef.current
    if (!el) return
    const obs = new IntersectionObserver(
      ([e]) => { if (e.isIntersecting) { setVisible(true); obs.disconnect() } },
      { threshold: 0.08 }
    )
    obs.observe(el)
    return () => obs.disconnect()
  }, [])

  return (
    <section
      ref={sectionRef}
      className="bg-zinc-950 py-20 px-6"
      style={{
        opacity: visible ? 1 : 0,
        transform: visible ? 'none' : 'translateY(28px)',
        transition: 'opacity 0.65s ease, transform 0.65s ease',
      }}
    >
      <div className="max-w-6xl mx-auto">
        <span className="text-xs font-mono tracking-[0.3em] text-zinc-500 uppercase">{t('landing.previewTitle')}</span>
        <p className="text-zinc-400 mt-3 mb-8 max-w-2xl leading-relaxed">{t('landing.previewDesc')}</p>

        <div className="rounded-xl border border-zinc-800 bg-zinc-900 overflow-hidden shadow-2xl">
          <div className="flex items-center gap-2 h-9 px-4 border-b border-zinc-800 bg-zinc-950/60">
            <span className="w-2.5 h-2.5 rounded-full bg-zinc-700" />
            <span className="w-2.5 h-2.5 rounded-full bg-zinc-700" />
            <span className="w-2.5 h-2.5 rounded-full bg-zinc-700" />
            <span className="mx-auto text-[11px] font-mono text-zinc-600 select-none">astraeusio.com/dashboard</span>
          </div>
          <div ref={frameRef} className="relative overflow-hidden pointer-events-none select-none" style={{ height }}>
            <div className="absolute top-0 left-0 origin-top-left" style={{ width: DESIGN_W, transform: `scale(${scale})` }}>
              <div ref={canvasRef}>
                <DashboardCanvas kpData={kpData} wind={wind} forecastData={forecastData} />
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
