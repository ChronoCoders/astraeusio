import { useTranslation } from 'react-i18next'

export default function ApodCard({ data }) {
  const { t } = useTranslation()

  if (!data) return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex items-center justify-center">
      <p className="text-zinc-600 text-sm">{t('common.noData')}</p>
    </div>
  )

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <div className="flex items-baseline justify-between">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('apod.title')}</span>
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
      ) : /\.(mp4|webm|ogg)(\?|$)/i.test(data.url) ? (
        <video
          src={data.url}
          controls
          className="w-full rounded"
          style={{ maxHeight: '280px' }}
        />
      ) : data.url?.includes('youtube.com/embed') ? (
        <div className="w-full rounded overflow-hidden" style={{ aspectRatio: '16/9' }}>
          <iframe
            src={data.url}
            title={data.title}
            className="w-full h-full"
            allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
            allowFullScreen
          />
        </div>
      ) : (
        <a
          href={data.url}
          target="_blank"
          rel="noreferrer"
          className="w-full bg-zinc-800 hover:bg-zinc-700 rounded flex flex-col items-center justify-center gap-2 transition-colors"
          style={{ height: '180px' }}
        >
          <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="text-zinc-400">
            <circle cx="12" cy="12" r="10" />
            <polygon points="10 8 16 12 10 16 10 8" fill="currentColor" stroke="none" />
          </svg>
          <span className="text-zinc-400 text-xs">{t('apod.viewMedia')}</span>
        </a>
      )}

      <h3 className="text-zinc-100 font-medium text-sm">{data.title}</h3>
      <p className="text-zinc-400 text-xs leading-relaxed line-clamp-4">{data.explanation}</p>
    </div>
  )
}
