import { useTranslation } from 'react-i18next'
import { stormInfo, kpBarColor } from '../lib/utils'

const GW = 200, GH = 120
const GCX = 100, GCY = 100
const GR  = 78
const GSW = 14

// Two explicit quarter-arcs: left(22,100) → top(100,22) → right(178,100).
// Each arc spans exactly 90° with sweep=1 (increasing θ in SVG formula),
// so there is no large/small arc ambiguity and the path unambiguously
// traces the top semicircle through (GCX, GCY-GR).
const TRACK = [
  `M ${GCX - GR} ${GCY}`,
  `A ${GR} ${GR} 0 0 1 ${GCX} ${GCY - GR}`,
  `A ${GR} ${GR} 0 0 1 ${GCX + GR} ${GCY}`,
].join(' ')

export default function KpGauge({ kp }) {
  const { t }  = useTranslation()
  const v      = kp != null ? Math.max(0, Math.min(9, kp)) : 0
  const color  = kpBarColor(v)
  const storm  = stormInfo(v)

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 h-full flex flex-col">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('charts.kpGauge')}</span>
      <div className="flex-1 flex items-center justify-center">
        <svg viewBox={`0 0 ${GW} ${GH}`} style={{ width: '100%', maxWidth: `${GW}px` }}>
          {/* Background track */}
          <path d={TRACK} fill="none" stroke="#27272a" strokeWidth={GSW} strokeLinecap="round" />
          {/* Fill — stroke-dasharray on pathLength=1 avoids any endpoint calculation */}
          {v > 0.05 && (
            <path
              d={TRACK}
              fill="none"
              stroke={color}
              strokeWidth={GSW}
              strokeLinecap="round"
              pathLength="1"
              strokeDasharray={`${(v / 9).toFixed(4)} 1`}
            />
          )}
          {/* Scale labels */}
          <text x={GCX - GR - 4} y={GCY + 5} textAnchor="end"   fill="#52525b" fontSize="10">0</text>
          <text x={GCX + GR + 4} y={GCY + 5} textAnchor="start" fill="#52525b" fontSize="10">9</text>
          {/* Current value */}
          <text x={GCX} y={GCY - 28} textAnchor="middle"
            fill={kp != null ? color : '#52525b'}
            fontSize="34" fontWeight="600" fontFamily="monospace">
            {kp != null ? kp.toFixed(1) : '—'}
          </text>
          {/* Storm level */}
          <text x={GCX} y={GCY - 10} textAnchor="middle" fill="#71717a" fontSize="11">
            {t(storm.key)}
          </text>
        </svg>
      </div>
    </div>
  )
}
