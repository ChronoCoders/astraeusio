import { useTranslation } from 'react-i18next'
import { stormInfo, kpBarColor } from '../lib/utils'

const GW = 200, GH = 120
const GCX = 100, GCY = 100   // center near bottom — arc spans upward
const GR  = 78               // arc radius
const GSW = 14               // stroke width

// Both arcs use CW (sweep=1) through the top semicircle.
// Screen angle formula: angle = (180 + (v/9)*180)°
// ex = GCX + GR*cos(angle), ey = GCY + GR*sin(angle)
const BG_PATH = `M ${GCX - GR} ${GCY} A ${GR} ${GR} 0 0 1 ${GCX + GR} ${GCY}`

function fillPath(v) {
  const a = (180 + (v / 9) * 180) * (Math.PI / 180)
  const ex = (GCX + GR * Math.cos(a)).toFixed(2)
  const ey = (GCY + GR * Math.sin(a)).toFixed(2)
  return `M ${GCX - GR} ${GCY} A ${GR} ${GR} 0 0 1 ${ex} ${ey}`
}

export default function KpGauge({ kp }) {
  const { t } = useTranslation()
  const v      = kp != null ? Math.max(0, Math.min(9, kp)) : 0
  const color  = kpBarColor(v)
  const storm  = stormInfo(v)

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 h-full flex flex-col">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('charts.kpGauge')}</span>
      <div className="flex-1 flex items-center justify-center">
        <svg viewBox={`0 0 ${GW} ${GH}`} style={{ width: '100%', maxWidth: `${GW}px` }}>
          {/* Background track */}
          <path d={BG_PATH} fill="none" stroke="#27272a" strokeWidth={GSW} strokeLinecap="round" />
          {/* Value fill */}
          {v > 0.05 && (
            <path d={fillPath(v)} fill="none" stroke={color} strokeWidth={GSW} strokeLinecap="round" />
          )}
          {/* End labels */}
          <text x={GCX - GR - 4} y={GCY + 5} textAnchor="end"   fill="#52525b" fontSize="10">0</text>
          <text x={GCX + GR + 4} y={GCY + 5} textAnchor="start" fill="#52525b" fontSize="10">9</text>
          {/* Current value */}
          <text x={GCX} y={GCY - 28} textAnchor="middle" fill={kp != null ? color : '#52525b'}
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
