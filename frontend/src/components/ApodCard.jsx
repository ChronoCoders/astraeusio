export default function ApodCard({ data }) {
  if (!data) return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex items-center justify-center">
      <p className="text-zinc-600 text-sm">No data</p>
    </div>
  )

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <div className="flex items-baseline justify-between">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">Astronomy Picture of the Day</span>
        <span className="text-zinc-500 text-xs font-mono">{data.date}</span>
      </div>

      {data.media_type === 'image' ? (
        <img
          src={data.url}
          alt={data.title}
          className="w-full rounded object-cover"
          style={{ maxHeight: '280px' }}
          loading="lazy"
        />
      ) : (
        <div className="w-full bg-zinc-800 rounded flex items-center justify-center" style={{ height: '180px' }}>
          <a href={data.url} target="_blank" rel="noreferrer" className="text-blue-400 text-sm underline">
            View media ↗
          </a>
        </div>
      )}

      <h3 className="text-zinc-100 font-medium text-sm">{data.title}</h3>
      <p className="text-zinc-400 text-xs leading-relaxed line-clamp-4">{data.explanation}</p>
    </div>
  )
}
