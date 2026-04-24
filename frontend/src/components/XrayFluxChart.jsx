import { useTranslation } from 'react-i18next'

const W = 560, H = 140
const PAD = { t: 16, r: 32, b: 28, l: 44 }
const CW = W - PAD.l - PAD.r
const CH = H - PAD.t - PAD.b

const Y_MIN = -8, Y_MAX = -3

const THRESHOLDS = [
  { log: -7, label: 'B' },
  { log: -6, label: 'C' },
  { log: -5, label: 'M', warn: true },
  { log: -4, label: 'X', crit: true },
]

export default function XrayFluxChart({ data }) {
  const { t } = useTranslation()

  const all = (data ?? [])
    .filter(r => r.energy === '0.1-0.8nm' && r.flux > 0)
    .sort((a, b) => new Date(a.time_tag) - new Date(b.time_tag))
  const latestMs = all.length ? new Date(all[all.length - 1].time_tag).getTime() : Date.now()
  const pts = all.filter(r => new Date(r.time_tag).getTime() >= latestMs - 86_400_000)

  if (pts.length < 2) return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 h-full flex flex-col">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('charts.xrayFlux')}</span>
      <div className="flex-1 flex items-center justify-center text-zinc-600 text-sm">{t('common.noData')}</div>
    </div>
  )

  const tArr = pts.map(r => new Date(r.time_tag).getTime())
  const tMin = tArr[0], tMax = tArr[tArr.length - 1]
  const tRange = tMax - tMin || 1

  const toX = ms  => PAD.l + ((ms - tMin) / tRange) * CW
  const toY = log => PAD.t + CH - ((log - Y_MIN) / (Y_MAX - Y_MIN)) * CH

  const polyline = pts.map((r, i) => {
    const log = Math.max(Y_MIN, Math.min(Y_MAX, Math.log10(r.flux)))
    return `${toX(tArr[i])},${toY(log)}`
  }).join(' ')

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 h-full flex flex-col">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('charts.xrayFlux')}</span>
      <svg viewBox={`0 0 ${W} ${H}`} className="w-full mt-2 flex-1" style={{ minHeight: `${H}px` }}>
        {THRESHOLDS.map(({ log, label, warn, crit }) => {
          const y = toY(log)
          if (y < PAD.t - 2 || y > PAD.t + CH + 2) return null
          const color = crit ? '#f87171' : warn ? '#fb923c' : '#52525b'
          return (
            <g key={log}>
              <line x1={PAD.l} y1={y} x2={PAD.l + CW} y2={y}
                stroke={color} strokeWidth="0.5" strokeDasharray="3 2" opacity="0.6" />
              <text x={PAD.l + CW + 3} y={y + 4} fill={color} fontSize="9" opacity="0.9">{label}</text>
            </g>
          )
        })}
        <polyline points={polyline} fill="none" stroke="#22d3ee" strokeWidth="1.5" strokeLinejoin="round" />
      </svg>
    </div>
  )
}
