import { useTranslation } from 'react-i18next'

const W = 560, H = 160
const PAD = { t: 16, r: 8, b: 28, l: 44 }
const CW = W - PAD.l - PAD.r
const CH = H - PAD.t - PAD.b

export default function ImfBzChart({ data }) {
  const { t } = useTranslation()

  const all = (data ?? [])
    .filter(r => r.bz_gsm != null)
    .sort((a, b) => new Date(a.time_tag) - new Date(b.time_tag))
  const latestMs = all.length ? new Date(all[all.length - 1].time_tag).getTime() : 0
  const pts = all.filter(r => new Date(r.time_tag).getTime() >= latestMs - 86_400_000)

  if (pts.length < 2) return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 h-full flex flex-col">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('charts.imfBz')}</span>
      <div className="flex-1 flex items-center justify-center text-zinc-600 text-sm">{t('common.noData')}</div>
    </div>
  )

  const vals = pts.map(r => r.bz_gsm)
  // Symmetric around zero — ensures 0 is always the visual midpoint.
  const absMax = Math.max(vals.reduce((a, b) => Math.max(a, Math.abs(b)), 0), 5) * 1.15
  const yMin = -absMax
  const yMax =  absMax

  const tArr = pts.map(r => new Date(r.time_tag).getTime())
  const tMin = tArr[0], tMax = tArr[tArr.length - 1]
  const tRange = tMax - tMin || 1

  const toX = ms => PAD.l + ((ms - tMin) / tRange) * CW
  const toY = v  => PAD.t + CH - ((v - yMin) / (yMax - yMin)) * CH

  const zeroY = toY(0)
  const polyline = pts.map((r, i) => `${toX(tArr[i])},${toY(r.bz_gsm)}`).join(' ')

  const areaPoints = [
    `${toX(tArr[0])},${zeroY}`,
    ...pts.map((r, i) => `${toX(tArr[i])},${toY(r.bz_gsm)}`),
    `${toX(tArr[tArr.length - 1])},${zeroY}`,
  ].join(' ')

  const aboveH = Math.max(0, zeroY - PAD.t)
  const belowH = Math.max(0, PAD.t + CH - zeroY)

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 h-full flex flex-col">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('charts.imfBz')}</span>
      <svg viewBox={`0 0 ${W} ${H}`} className="w-full mt-2 flex-1" style={{ minHeight: `${H}px` }}>
        <defs>
          <clipPath id="imf-above">
            <rect x={PAD.l} y={PAD.t} width={CW} height={aboveH} />
          </clipPath>
          <clipPath id="imf-below">
            <rect x={PAD.l} y={zeroY} width={CW} height={belowH} />
          </clipPath>
        </defs>

        {/* Grid lines: min, 0, max */}
        {[yMin, 0, yMax].map((v, i) => {
          const y = toY(v)
          const isZero = v === 0
          return (
            <g key={i}>
              <line
                x1={PAD.l} y1={y} x2={PAD.l + CW} y2={y}
                stroke={isZero ? '#3f3f46' : '#27272a'} strokeWidth={isZero ? 1 : 1}
              />
              <text x={PAD.l - 4} y={y + 4} textAnchor="end"
                fill={isZero ? '#71717a' : '#52525b'} fontSize="10">
                {Math.round(v)}
              </text>
            </g>
          )
        })}

        {/* Northward fill (positive Bz — blue) */}
        <polygon points={areaPoints} fill="#3b82f6" fillOpacity="0.18"
          clipPath="url(#imf-above)" />
        {/* Southward fill (negative Bz — red) */}
        <polygon points={areaPoints} fill="#ef4444" fillOpacity="0.18"
          clipPath="url(#imf-below)" />

        {/* Line — northward portion blue */}
        <polyline points={polyline} fill="none" stroke="#60a5fa" strokeWidth="1.5"
          strokeLinejoin="round" clipPath="url(#imf-above)" />
        {/* Line — southward portion red */}
        <polyline points={polyline} fill="none" stroke="#f87171" strokeWidth="1.5"
          strokeLinejoin="round" clipPath="url(#imf-below)" />
      </svg>
    </div>
  )
}
