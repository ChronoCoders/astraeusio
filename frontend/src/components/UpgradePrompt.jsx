import { useTranslation } from 'react-i18next'

export default function UpgradePrompt({ messageKey, requiredPlan }) {
  const { t } = useTranslation()

  return (
    <div className="flex flex-col items-center justify-center py-16 gap-4 text-center">
      <svg width="32" height="32" viewBox="0 0 24 24" fill="none"
        stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"
        className="text-zinc-600">
        <rect x="3" y="11" width="18" height="11" rx="2" />
        <path d="M7 11V7a5 5 0 0 1 10 0v4" />
      </svg>
      <div className="flex flex-col gap-1">
        <p className="text-zinc-300 text-sm font-mono">{t('plan.upgradeRequired')}</p>
        <p className="text-zinc-500 text-xs max-w-xs">{t(messageKey)}</p>
      </div>
      {requiredPlan && (
        <span className="text-xs font-mono border border-blue-800 text-blue-400 rounded px-2 py-0.5">
          {t(`plan.${requiredPlan}`)}+
        </span>
      )}
      <a
        href="/pricing"
        target="_blank"
        rel="noopener noreferrer"
        className="text-xs font-mono text-zinc-500 hover:text-zinc-300 transition-colors underline underline-offset-2"
      >
        {t('plan.upgradeBtn')} →
      </a>
    </div>
  )
}
