import { useState } from 'react'
import { epicImageUrl } from '../lib/utils'

export default function EpicViewer({ data }) {
  const images = (data ?? []).slice(0, 4)
  const [active, setActive] = useState(0)
  const img = images[active]

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <div className="flex items-baseline justify-between">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">EPIC — Earth Polychromatic Imaging</span>
        {img && <span className="text-zinc-500 text-xs font-mono">{img.date?.slice(0, 10)}</span>}
      </div>

      {!img ? (
        <div className="flex items-center justify-center bg-zinc-800 rounded" style={{ height: '260px' }}>
          <p className="text-zinc-600 text-sm">No data</p>
        </div>
      ) : (
        <>
          <img
            src={epicImageUrl(img)}
            alt="Earth from DSCOVR/EPIC"
            className="w-full rounded object-contain bg-black"
            style={{ maxHeight: '260px' }}
            loading="lazy"
          />

          <p className="text-zinc-400 text-xs leading-relaxed line-clamp-2">{img.caption}</p>

          <div className="flex items-center gap-1.5">
            <span className="text-zinc-600 text-xs mr-1">Frame:</span>
            {images.map((_, i) => (
              <button
                key={i}
                onClick={() => setActive(i)}
                className={`w-2 h-2 rounded-full transition-colors ${i === active ? 'bg-blue-400' : 'bg-zinc-700 hover:bg-zinc-500'}`}
              />
            ))}
            {img.centroid_coordinates && (
              <span className="text-zinc-600 text-xs ml-auto">
                {img.centroid_coordinates.lat?.toFixed(1)}°N {img.centroid_coordinates.lon?.toFixed(1)}°E
              </span>
            )}
          </div>
        </>
      )}
    </div>
  )
}
