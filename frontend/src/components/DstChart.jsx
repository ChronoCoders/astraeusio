import { useTranslation } from 'react-i18next'

const W = 560, H = 140
const PAD = { t: 16, r: 8, b: 28, l: 44 }
const CW = W - PAD.l - PAD.r
const CH = H - PAD.t - PAD.b

const THRESHOLDS = [
  { v: -30,  labelKey: 'charts.dstModerate', color: '#f59e0b' },
  { v: -100, labelKey: 'charts.dstSevere',   color: '#ef4444' },
]

export default function DstChart({ data }) {
  const { t } = useTranslation()

  const all = (data ?? [])
    .filter(r => r.dst_nt != null)
    .sort((a, b) => new Date(a.time_tag) - new Date(b.time_tag))
  const latestMs = all.length ? new Date(all[all.length - 1].time_tag).getTime() : 0
  const pts = all.filter(r => new Date(r.time_tag).getTime() >= latestMs - 7 * 86_400_000)

  if (pts.length < 2) return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 h-full flex flex-col">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('charts.dst')}</span>
      <div className="flex-1 flex items-center justify-center text-zinc-600 text-sm">{t('common.noData')}</div>
    </div>
  )

  const vals = pts.map(r => r.dst_nt)
  const minV = vals.reduce((a, b) => Math.min(a, b), Infinity)
  const maxV = vals.reduce((a, b) => Math.max(a, b), -Infinity)
  const pad  = (maxV - minV) * 0.1 || 10
  // Always include -110 so the -100 nT threshold line is visible.
  const yMin = Math.min(minV - pad, -110)
  const yMax = Math.max(maxV + pad, 20)

  const tArr = pts.map(r => new Date(r.time_tag).getTime())
  const tMin = tArr[0], tMax = tArr[tArr.length - 1]
  const tRange = tMax - tMin || 1

  const toX = ms => PAD.l + ((ms - tMin) / tRange) * CW
  const toY = v  => PAD.t + CH - ((v - yMin) / (yMax - yMin)) * CH

  const polyline = pts.map((r, i) => `${toX(tArr[i])},${toY(r.dst_nt)}`).join(' ')
  const yTicks   = [yMin, (yMin + yMax) / 2, yMax]

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 h-full flex flex-col">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('charts.dst')}</span>
      <svg viewBox={`0 0 ${W} ${H}`} className="w-full mt-2 flex-1" style={{ minHeight: `${H}px` }}>

        {/* Grid lines */}
        {yTicks.map((v, i) => {
          const y = toY(v)
          return (
            <g key={i}>
              <line x1={PAD.l} y1={y} x2={PAD.l + CW} y2={y} stroke="#27272a" strokeWidth="1" />
              <text x={PAD.l - 4} y={y + 4} textAnchor="end" fill="#52525b" fontSize="10">
                {Math.round(v)}
              </text>
            </g>
          )
        })}

        {/* Storm threshold lines */}
        {THRESHOLDS.map(({ v, labelKey, color }) => {
          const y = toY(v)
          if (y < PAD.t - 2 || y > PAD.t + CH + 2) return null
          return (
            <g key={v}>
              <line x1={PAD.l} y1={y} x2={PAD.l + CW} y2={y}
                stroke={color} strokeWidth="1" strokeDasharray="4 2" />
              <text x={PAD.l + 4} y={y - 3} fill={color} fontSize="9" opacity="0.9">
                {t(labelKey)}
              </text>
            </g>
          )
        })}

        <polyline points={polyline} fill="none" stroke="#22d3ee" strokeWidth="1.5"
          strokeLinejoin="round" />
      </svg>
    </div>
  )
}
