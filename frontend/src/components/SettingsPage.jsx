import { useState } from 'react'
import { useTranslation } from 'react-i18next'

const LANGS = ['en', 'tr']

function authHeader() {
  return { Authorization: `Bearer ${localStorage.getItem('token')}` }
}

export default function SettingsPage({ onLogout }) {
  const { t, i18n } = useTranslation()

  const [current, setCurrent]     = useState('')
  const [newPw, setNewPw]         = useState('')
  const [confirm, setConfirm]     = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [pwError, setPwError]     = useState(null)
  const [pwSuccess, setPwSuccess] = useState(false)

  async function handleChangePassword(e) {
    e.preventDefault()
    setPwError(null)
    setPwSuccess(false)

    if (newPw !== confirm) {
      setPwError(t('auth.passwordMismatch'))
      return
    }
    if (newPw.length < 8) {
      setPwError(t('settings.pwTooShort'))
      return
    }

    setSubmitting(true)
    try {
      const r = await fetch('/auth/change-password', {
        method: 'POST',
        headers: { ...authHeader(), 'Content-Type': 'application/json' },
        body: JSON.stringify({ current_password: current, new_password: newPw }),
      })
      if (r.status === 204) {
        setPwSuccess(true)
        setCurrent('')
        setNewPw('')
        setConfirm('')
      } else {
        const d = await r.json().catch(() => ({}))
        setPwError(d.error ?? t('auth.unknownError'))
      }
    } catch {
      setPwError(t('auth.networkError'))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="flex flex-col gap-6 max-w-lg">

      {/* ── Language ────────────────────────────────────────────────── */}
      <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
        <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">
          {t('settings.languageTitle')}
        </span>
        <div className="flex items-center gap-2">
          {LANGS.map(lng => (
            <button
              key={lng}
              onClick={() => i18n.changeLanguage(lng)}
              className={[
                'px-4 py-1.5 text-xs font-mono rounded border transition-colors',
                i18n.language === lng
                  ? 'bg-zinc-700 border-zinc-600 text-zinc-100'
                  : 'border-zinc-700 text-zinc-500 hover:text-zinc-300 hover:border-zinc-500',
              ].join(' ')}
            >
              {lng.toUpperCase()}
            </button>
          ))}
          <span className="text-zinc-600 text-xs font-mono ml-2">
            {i18n.language === 'tr' ? 'Türkçe' : 'English'}
          </span>
        </div>
      </div>

      {/* ── Change password ──────────────────────────────────────────── */}
      <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
        <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">
          {t('settings.changePasswordTitle')}
        </span>
        <form onSubmit={handleChangePassword} className="flex flex-col gap-3">
          <div className="flex flex-col gap-1">
            <label className="text-zinc-600 text-xs font-mono">{t('settings.currentPassword')}</label>
            <input
              type="password"
              value={current}
              onChange={e => setCurrent(e.target.value)}
              required
              className="bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-200 font-mono focus:outline-none focus:border-zinc-500"
            />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-zinc-600 text-xs font-mono">{t('settings.newPassword')}</label>
            <input
              type="password"
              value={newPw}
              onChange={e => setNewPw(e.target.value)}
              required
              className="bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-200 font-mono focus:outline-none focus:border-zinc-500"
            />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-zinc-600 text-xs font-mono">{t('settings.confirmNewPassword')}</label>
            <input
              type="password"
              value={confirm}
              onChange={e => setConfirm(e.target.value)}
              required
              className="bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-200 font-mono focus:outline-none focus:border-zinc-500"
            />
          </div>
          {pwError && <p className="text-red-400 text-xs font-mono">{pwError}</p>}
          {pwSuccess && <p className="text-green-400 text-xs font-mono">{t('settings.pwChanged')}</p>}
          <button
            type="submit"
            disabled={submitting}
            className="self-start px-4 py-2 text-xs font-mono rounded bg-zinc-700 text-zinc-100 hover:bg-zinc-600 disabled:bg-zinc-800 disabled:text-zinc-600 transition-colors"
          >
            {submitting ? t('common.loading') : t('settings.changePasswordBtn')}
          </button>
        </form>
      </div>

      {/* ── Sign out ─────────────────────────────────────────────────── */}
      <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
        <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">
          {t('settings.sessionTitle')}
        </span>
        <button
          onClick={onLogout}
          className="self-start px-4 py-2 text-xs font-mono rounded border border-zinc-700 text-zinc-400 hover:text-red-400 hover:border-red-800 transition-colors"
        >
          {t('auth.logout')}
        </button>
      </div>

    </div>
  )
}
