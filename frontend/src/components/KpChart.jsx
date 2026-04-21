import { useTranslation } from 'react-i18next'
import { processKpBuckets, kpBarColor } from '../lib/utils'

const W = 560
const H = 140
const PAD = { t: 16, r: 8, b: 28, l: 28 }
const CW = W - PAD.l - PAD.r
const CH = H - PAD.t - PAD.b
const MAX_KP = 9

export default function KpChart({ records }) {
  const { t } = useTranslation()
  const bars = processKpBuckets(records)

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 h-full flex flex-col">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('kpChart.title')}</span>
      {bars.length === 0 ? (
        <div className="flex-1 flex items-center justify-center text-zinc-600 text-sm mt-2">{t('common.noData')}</div>
      ) : (
        <svg viewBox={`0 0 ${W} ${H}`} className="w-full mt-2 flex-1" style={{ minHeight: `${H}px` }}>
          {/* Gridlines at Kp 3, 5, 7 */}
          {[3, 5, 7].map(y => {
            const cy = PAD.t + CH - (y / MAX_KP) * CH
            return (
              <g key={y}>
                <line x1={PAD.l} y1={cy} x2={PAD.l + CW} y2={cy} stroke="#27272a" strokeWidth="1" />
                <text x={PAD.l - 4} y={cy + 4} textAnchor="end" fill="#52525b" fontSize="11">{y}</text>
              </g>
            )
          })}
          {/* Y-axis min/max labels */}
          <text x={PAD.l - 4} y={PAD.t + CH + 4} textAnchor="end" fill="#52525b" fontSize="11">0</text>
          <text x={PAD.l - 4} y={PAD.t + 4}      textAnchor="end" fill="#52525b" fontSize="11">9</text>

          {/* Bars */}
          {bars.map((bar, i) => {
            const slotW = CW / bars.length
            const barW  = slotW * 0.72
            const x     = PAD.l + i * slotW + (slotW - barW) / 2
            const bh    = (bar.kp / MAX_KP) * CH
            const y     = PAD.t + CH - bh
            const label = `${String(bar.h).padStart(2, '0')}:00`
            return (
              <g key={i}>
                <rect x={x} y={y} width={barW} height={Math.max(bh, 1)} fill={kpBarColor(bar.kp)} rx="1" />
                <text x={x + barW / 2} y={H - 4} textAnchor="middle" fill="#52525b" fontSize="10">{label}</text>
              </g>
            )
          })}
        </svg>
      )}
    </div>
  )
}
