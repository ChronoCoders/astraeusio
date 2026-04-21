import { useState } from 'react'
import { useTranslation } from 'react-i18next'

const LANGS = ['en', 'tr']

export default function AuthPage({ onAuth }) {
  const { t, i18n } = useTranslation()
  const [mode, setMode]       = useState('login')
  const [email, setEmail]     = useState('')
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [error, setError]     = useState(null)
  const [loading, setLoading] = useState(false)

  function switchMode(next) {
    setMode(next)
    setError(null)
    setConfirm('')
  }

  async function submit(e) {
    e.preventDefault()
    if (mode === 'signup' && password !== confirm) {
      setError(t('auth.passwordMismatch'))
      return
    }
    setLoading(true)
    setError(null)
    try {
      const endpoint = mode === 'login' ? '/auth/login' : '/auth/register'
      const res = await fetch(endpoint, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, password }),
      })

      if (!res.ok) {
        let msg = t('auth.unknownError')
        try { msg = (await res.json()).error ?? msg } catch { /* non-JSON body */ }
        setError(msg)
        return
      }

      if (mode === 'signup') {
        // 201 Created — auto-login
        const loginRes = await fetch('/auth/login', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ email, password }),
        })
        if (!loginRes.ok) {
          let msg = t('auth.unknownError')
          try { msg = (await loginRes.json()).error ?? msg } catch { /* non-JSON body */ }
          setError(msg)
          return
        }
        const { token } = await loginRes.json()
        localStorage.setItem('token', token)
        onAuth(token)
      } else {
        const { token } = await res.json()
        localStorage.setItem('token', token)
        onAuth(token)
      }
    } catch {
      setError(t('auth.networkError'))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="min-h-screen flex">

      {/* ── Left: space background ───────────────────────────────────────── */}
      <div className="hidden lg:flex lg:w-1/2 xl:w-3/5 space-panel relative flex-col items-center justify-center select-none">
        <div className="star-field" />

        {/* Glow orbs */}
        <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
          <div
            className="w-80 h-80 rounded-full blur-3xl"
            style={{
              background: 'radial-gradient(circle, rgba(59,130,246,0.08) 0%, transparent 70%)',
              animation: 'pulse-glow 8s ease-in-out infinite',
            }}
          />
          <div
            className="absolute w-48 h-48 rounded-full blur-2xl"
            style={{
              background: 'radial-gradient(circle, rgba(139,92,246,0.12) 0%, transparent 70%)',
              animation: 'pulse-glow 12s ease-in-out infinite 3s',
            }}
          />
        </div>

        {/* Wordmark */}
        <div className="relative z-10 text-center px-12">
          <p className="text-zinc-600 text-xs tracking-[0.4em] uppercase mb-4">{t('auth.tagline1')}</p>
          <h1 className="text-5xl font-thin tracking-[0.25em] text-zinc-100 mb-3">ASTRAEUSIO</h1>
          <p className="text-zinc-500 text-xs tracking-widest">{t('auth.tagline2')}</p>
        </div>
      </div>

      {/* ── Right: form ──────────────────────────────────────────────────── */}
      <div className="flex-1 flex flex-col bg-zinc-950">

        {/* Top bar — lang toggle */}
        <div className="flex justify-end p-4">
          <div className="flex items-center gap-1">
            {LANGS.map(lng => (
              <button
                key={lng}
                onClick={() => i18n.changeLanguage(lng)}
                className={`text-xs font-mono px-2 py-0.5 rounded transition-colors ${
                  i18n.language === lng
                    ? 'bg-zinc-700 text-zinc-100'
                    : 'text-zinc-500 hover:text-zinc-300'
                }`}
              >
                {lng.toUpperCase()}
              </button>
            ))}
          </div>
        </div>

        {/* Centered form */}
        <div className="flex-1 flex items-center justify-center p-8">
          <div className="w-full max-w-sm">

            {/* Mobile wordmark */}
            <p className="lg:hidden text-zinc-100 font-thin tracking-widest text-2xl mb-8 text-center">
              ASTRAEUSIO
            </p>

            <h2 className="text-zinc-100 text-xl font-semibold mb-1">
              {mode === 'login' ? t('auth.loginTitle') : t('auth.signupTitle')}
            </h2>
            <p className="text-zinc-500 text-sm mb-7">
              {mode === 'login' ? t('auth.loginSub') : t('auth.signupSub')}
            </p>

            <form onSubmit={submit} className="flex flex-col gap-4">
              <Field label={t('auth.email')}>
                <input
                  type="email"
                  value={email}
                  onChange={e => setEmail(e.target.value)}
                  required
                  autoComplete="email"
                  placeholder="you@example.com"
                  className="w-full bg-zinc-900 border border-zinc-800 rounded px-3 py-2 text-zinc-100 text-sm placeholder-zinc-600 focus:outline-none focus:border-zinc-600 transition-colors"
                />
              </Field>

              <Field label={t('auth.password')}>
                <input
                  type="password"
                  value={password}
                  onChange={e => setPassword(e.target.value)}
                  required
                  autoComplete={mode === 'login' ? 'current-password' : 'new-password'}
                  placeholder="••••••••"
                  className="w-full bg-zinc-900 border border-zinc-800 rounded px-3 py-2 text-zinc-100 text-sm placeholder-zinc-600 focus:outline-none focus:border-zinc-600 transition-colors"
                />
              </Field>

              {mode === 'signup' && (
                <Field label={t('auth.confirmPassword')}>
                  <input
                    type="password"
                    value={confirm}
                    onChange={e => setConfirm(e.target.value)}
                    required
                    autoComplete="new-password"
                    placeholder="••••••••"
                    className="w-full bg-zinc-900 border border-zinc-800 rounded px-3 py-2 text-zinc-100 text-sm placeholder-zinc-600 focus:outline-none focus:border-zinc-600 transition-colors"
                  />
                </Field>
              )}

              {error && (
                <p className="text-red-400 text-xs bg-red-950/40 border border-red-900/40 rounded px-3 py-2">
                  {error}
                </p>
              )}

              <button
                type="submit"
                disabled={loading}
                className="bg-zinc-100 text-zinc-900 font-medium text-sm rounded px-4 py-2.5 hover:bg-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed mt-1"
              >
                {loading
                  ? t('common.loading')
                  : mode === 'login'
                  ? t('auth.loginBtn')
                  : t('auth.signupBtn')}
              </button>
            </form>

            <p className="text-zinc-600 text-xs text-center mt-6">
              {mode === 'login' ? (
                <>
                  {t('auth.noAccount')}{' '}
                  <button
                    onClick={() => switchMode('signup')}
                    className="text-zinc-400 hover:text-zinc-200 underline-offset-2 hover:underline transition-colors"
                  >
                    {t('auth.switchToSignup')}
                  </button>
                </>
              ) : (
                <>
                  {t('auth.hasAccount')}{' '}
                  <button
                    onClick={() => switchMode('login')}
                    className="text-zinc-400 hover:text-zinc-200 underline-offset-2 hover:underline transition-colors"
                  >
                    {t('auth.switchToLogin')}
                  </button>
                </>
              )}
            </p>
          </div>
        </div>
      </div>
    </div>
  )
}

function Field({ label, children }) {
  return (
    <div className="flex flex-col gap-1.5">
      <label className="text-zinc-400 text-xs">{label}</label>
      {children}
    </div>
  )
}
