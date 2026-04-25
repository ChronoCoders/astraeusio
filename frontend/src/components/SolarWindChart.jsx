import { useTranslation } from 'react-i18next'

const W = 560, H = 140
const PAD = { t: 16, r: 8, b: 28, l: 44 }
const CW = W - PAD.l - PAD.r
const CH = H - PAD.t - PAD.b

export default function SolarWindChart({ data }) {
  const { t } = useTranslation()

  const all = (data ?? [])
    .filter(r => r.proton_speed != null)
    .sort((a, b) => new Date(a.time_tag) - new Date(b.time_tag))
  const latestMs = all.length ? new Date(all[all.length - 1].time_tag).getTime() : 0
  const pts = all.filter(r => new Date(r.time_tag).getTime() >= latestMs - 86_400_000)

  if (pts.length < 2) return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 h-full flex flex-col">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('charts.solarWind')}</span>
      <div className="flex-1 flex items-center justify-center text-zinc-600 text-sm">{t('common.noData')}</div>
    </div>
  )

  const speeds = pts.map(r => r.proton_speed)
  const minV = speeds.reduce((a, b) => Math.min(a, b), Infinity)
  const maxV = speeds.reduce((a, b) => Math.max(a, b), -Infinity)
  const padding = (maxV - minV) * 0.1 || 50
  const yMin = Math.max(0, minV - padding)
  const yMax = maxV + padding

  const tArr = pts.map(r => new Date(r.time_tag).getTime())
  const tMin = tArr[0], tMax = tArr[tArr.length - 1]
  const tRange = tMax - tMin || 1

  const toX = ms => PAD.l + ((ms - tMin) / tRange) * CW
  const toY = v  => PAD.t + CH - ((v - yMin) / (yMax - yMin)) * CH

  const polyline = pts.map((r, i) => `${toX(tArr[i])},${toY(r.proton_speed)}`).join(' ')
  const yTicks = [yMin, (yMin + yMax) / 2, yMax]
  const warnInRange = 700 >= yMin && 700 <= yMax
  const warnY = warnInRange ? toY(700) : null

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 h-full flex flex-col">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('charts.solarWind')}</span>
      <svg viewBox={`0 0 ${W} ${H}`} className="w-full mt-2 flex-1" style={{ minHeight: `${H}px` }}>
        {yTicks.map((v, i) => {
          const y = toY(v)
          return (
            <g key={i}>
              <line x1={PAD.l} y1={y} x2={PAD.l + CW} y2={y} stroke="#27272a" strokeWidth="1" />
              <text x={PAD.l - 4} y={y + 4} textAnchor="end" fill="#52525b" fontSize="10">{Math.round(v)}</text>
            </g>
          )
        })}
        {warnY != null && (
          <line x1={PAD.l} y1={warnY} x2={PAD.l + CW} y2={warnY}
            stroke="#f59e0b" strokeWidth="1" strokeDasharray="4 2" />
        )}
        <polyline points={polyline} fill="none" stroke="#22d3ee" strokeWidth="1.5" strokeLinejoin="round" />
      </svg>
    </div>
  )
}
