import { useState, useEffect } from 'react'
import { useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'

const LANGS = ['en', 'tr']

export default function AuthPage({ onAuth, initialMode = 'login', initialPartialToken = null, oauthErrorCode = null }) {
  const { t, i18n } = useTranslation()
  const [searchParams] = useSearchParams()
  const [mode, setMode]           = useState(initialMode)
  const [email, setEmail]         = useState('')
  const [password, setPassword]   = useState('')
  const [confirm, setConfirm]     = useState('')
  const [error, setError]         = useState(oauthErrorCode ? t(`auth.oauthError.${oauthErrorCode}`, t('auth.oauthError.oauth_failed')) : null)
  const [success, setSuccess]     = useState(null)
  const [loading, setLoading]     = useState(false)
  // 2FA step (may be pre-seeded by an OAuth login that requires TOTP)
  const [partialToken, setPartial] = useState(initialPartialToken)
  const [totpCode, setTotpCode]   = useState('')
  // Configured social-login providers (empty until fetched).
  const [providers, setProviders] = useState([])

  useEffect(() => {
    let cancelled = false
    fetch('/api/auth/providers')
      .then(r => r.ok ? r.json() : null)
      .then(d => { if (!cancelled && d?.providers) setProviders(d.providers) })
      .catch(() => {})
    return () => { cancelled = true }
  }, [])

  function switchMode(next) {
    setMode(next)
    setError(null)
    setSuccess(null)
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
        setLoading(false)
        setError(msg)
        return
      }

      if (mode === 'signup') {
        setLoading(false)
        setMode('login')
        setConfirm('')
        setPassword('')
        setSuccess(t('auth.accountCreated'))
        return
      }

      const data = await res.json()
      if (data.requires_2fa) {
        setLoading(false)
        setPartial(data.partial_token)
        setError(null)
        return
      }

      // Login success — keep loading=true so button stays disabled while
      // Root transitions to App. AuthPage unmounts; no setLoading(false) needed.
      onAuth(data.token)
    } catch {
      setLoading(false)
      setError(t('auth.networkError'))
    }
  }

  async function submitForgot(e) {
    e.preventDefault()
    setLoading(true)
    setError(null)
    try {
      await fetch('/auth/forgot-password', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email }),
      })
      setLoading(false)
      setSuccess(t('auth.forgotSent'))
    } catch {
      setLoading(false)
      setError(t('auth.networkError'))
    }
  }

  async function submitReset(e) {
    e.preventDefault()
    if (password !== confirm) {
      setError(t('auth.passwordMismatch'))
      return
    }
    setLoading(true)
    setError(null)
    try {
      const token = searchParams.get('token') ?? ''
      const res = await fetch('/auth/reset-password', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ token, new_password: password }),
      })
      if (!res.ok) {
        let msg = t('auth.unknownError')
        try { msg = (await res.json()).error ?? msg } catch { /* non-JSON body */ }
        setLoading(false)
        setError(msg)
        return
      }
      setLoading(false)
      setPassword('')
      setConfirm('')
      setSuccess(t('auth.resetSuccess'))
      setTimeout(() => switchMode('login'), 2000)
    } catch {
      setLoading(false)
      setError(t('auth.networkError'))
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

        {/* ── 2FA step ─────────────────────────────────────────────────── */}
        {partialToken && (
          <div className="flex-1 flex items-center justify-center p-8">
            <div className="w-full max-w-sm">
              <h2 className="text-zinc-100 text-xl font-semibold mb-1">{t('auth.twoFactorTitle')}</h2>
              <p className="text-zinc-500 text-sm mb-7">{t('auth.twoFactorSubtitle')}</p>
              <form onSubmit={async e => {
                e.preventDefault()
                setLoading(true)
                setError(null)
                try {
                  const r = await fetch('/auth/2fa/login', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ partial_token: partialToken, code: totpCode }),
                  })
                  if (!r.ok) {
                    const d = await r.json().catch(() => ({}))
                    setError(d.error ?? t('auth.unknownError'))
                    setLoading(false)
                    return
                  }
                  const { token } = await r.json()
                  onAuth(token)
                } catch {
                  setError(t('auth.networkError'))
                  setLoading(false)
                }
              }} className="flex flex-col gap-4">
                <Field label={t('auth.authenticatorCode')}>
                  <input
                    type="text"
                    name="totp-code"
                    inputMode="numeric"
                    maxLength={6}
                    value={totpCode}
                    onChange={e => setTotpCode(e.target.value.replace(/\D/g, ''))}
                    required
                    autoFocus
                    placeholder="000000"
                    className="w-full bg-zinc-900 border border-zinc-800 rounded px-3 py-2 text-zinc-100 text-sm placeholder-zinc-600 focus:outline-none focus:border-zinc-600 transition-colors tracking-widest"
                  />
                </Field>
                {error && <p className="text-red-400 text-xs bg-red-950/40 border border-red-900/40 rounded px-3 py-2">{error}</p>}
                <button
                  type="submit"
                  disabled={loading || totpCode.length !== 6}
                  className="bg-zinc-100 text-zinc-900 font-medium text-sm rounded px-4 py-2.5 hover:bg-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed mt-1"
                >
                  {loading ? t('common.loading') : t('auth.verify')}
                </button>
              </form>
              <button onClick={() => { setPartial(null); setTotpCode(''); setError(null) }}
                className="mt-4 text-xs text-zinc-600 hover:text-zinc-400 transition-colors">
                {t('auth.backToLogin')}
              </button>
            </div>
          </div>
        )}

        {/* ── Forgot password form ─────────────────────────────────────── */}
        {!partialToken && mode === 'forgot' && (
          <div className="flex-1 flex items-center justify-center p-8">
            <div className="w-full max-w-sm">
              <p className="lg:hidden text-zinc-100 font-thin tracking-widest text-2xl mb-8 text-center">ASTRAEUSIO</p>
              <h2 className="text-zinc-100 text-xl font-semibold mb-1">{t('auth.forgotTitle')}</h2>
              <p className="text-zinc-500 text-sm mb-7">{t('auth.forgotSub')}</p>
              <form onSubmit={submitForgot} className="flex flex-col gap-4">
                <Field label={t('auth.email')}>
                  <input
                    type="email"
                    name="email"
                    value={email}
                    onChange={e => setEmail(e.target.value)}
                    required
                    autoComplete="email"
                    placeholder="you@example.com"
                    className="w-full bg-zinc-900 border border-zinc-800 rounded px-3 py-2 text-zinc-100 text-sm placeholder-zinc-600 focus:outline-none focus:border-zinc-600 transition-colors"
                  />
                </Field>
                {success && <p className="text-green-400 text-xs bg-green-950/40 border border-green-900/40 rounded px-3 py-2">{success}</p>}
                {error && <p className="text-red-400 text-xs bg-red-950/40 border border-red-900/40 rounded px-3 py-2">{error}</p>}
                <button
                  type="submit"
                  disabled={loading || !!success}
                  className="bg-zinc-100 text-zinc-900 font-medium text-sm rounded px-4 py-2.5 hover:bg-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed mt-1"
                >
                  {loading ? t('common.loading') : t('auth.forgotBtn')}
                </button>
              </form>
              <button onClick={() => switchMode('login')} className="mt-4 text-xs text-zinc-600 hover:text-zinc-400 transition-colors">
                {t('auth.backToSignIn')}
              </button>
            </div>
          </div>
        )}

        {/* ── Reset password form ──────────────────────────────────────── */}
        {!partialToken && mode === 'reset' && (
          <div className="flex-1 flex items-center justify-center p-8">
            <div className="w-full max-w-sm">
              <p className="lg:hidden text-zinc-100 font-thin tracking-widest text-2xl mb-8 text-center">ASTRAEUSIO</p>
              <h2 className="text-zinc-100 text-xl font-semibold mb-1">{t('auth.resetTitle')}</h2>
              <p className="text-zinc-500 text-sm mb-7">{t('auth.resetSub')}</p>
              <form onSubmit={submitReset} className="flex flex-col gap-4">
                <Field label={t('auth.newPassword')}>
                  <input
                    type="password"
                    name="new-password"
                    value={password}
                    onChange={e => setPassword(e.target.value)}
                    required
                    autoComplete="new-password"
                    placeholder="••••••••"
                    className="w-full bg-zinc-900 border border-zinc-800 rounded px-3 py-2 text-zinc-100 text-sm placeholder-zinc-600 focus:outline-none focus:border-zinc-600 transition-colors"
                  />
                </Field>
                <Field label={t('auth.confirmPassword')}>
                  <input
                    type="password"
                    name="confirm-new-password"
                    value={confirm}
                    onChange={e => setConfirm(e.target.value)}
                    required
                    autoComplete="new-password"
                    placeholder="••••••••"
                    className="w-full bg-zinc-900 border border-zinc-800 rounded px-3 py-2 text-zinc-100 text-sm placeholder-zinc-600 focus:outline-none focus:border-zinc-600 transition-colors"
                  />
                </Field>
                {success && <p className="text-green-400 text-xs bg-green-950/40 border border-green-900/40 rounded px-3 py-2">{success}</p>}
                {error && <p className="text-red-400 text-xs bg-red-950/40 border border-red-900/40 rounded px-3 py-2">{error}</p>}
                <button
                  type="submit"
                  disabled={loading || !!success}
                  className="bg-zinc-100 text-zinc-900 font-medium text-sm rounded px-4 py-2.5 hover:bg-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed mt-1"
                >
                  {loading ? t('common.loading') : t('auth.resetBtn')}
                </button>
              </form>
            </div>
          </div>
        )}

        {/* Centered form */}
        {!partialToken && (mode === 'login' || mode === 'signup') && <div className="flex-1 flex items-center justify-center p-8">
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

            {providers.length > 0 && (
              <>
                <div className="flex flex-col gap-2 mb-5">
                  {providers.includes('github') && <SocialButton provider="github" label={t('auth.continueGithub')} />}
                  {providers.includes('google') && <SocialButton provider="google" label={t('auth.continueGoogle')} />}
                </div>
                <div className="flex items-center gap-3 mb-5">
                  <span className="h-px flex-1 bg-zinc-800" />
                  <span className="text-zinc-600 text-xs">{t('auth.orEmail')}</span>
                  <span className="h-px flex-1 bg-zinc-800" />
                </div>
              </>
            )}

            <form onSubmit={submit} className="flex flex-col gap-4">
              <Field label={t('auth.email')}>
                <input
                  type="email"
                  name="email"
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
                  name="password"
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
                    name="confirm-password"
                    value={confirm}
                    onChange={e => setConfirm(e.target.value)}
                    required
                    autoComplete="new-password"
                    placeholder="••••••••"
                    className="w-full bg-zinc-900 border border-zinc-800 rounded px-3 py-2 text-zinc-100 text-sm placeholder-zinc-600 focus:outline-none focus:border-zinc-600 transition-colors"
                  />
                </Field>
              )}

              {success && (
                <p className="text-green-400 text-xs bg-green-950/40 border border-green-900/40 rounded px-3 py-2">
                  {success}
                </p>
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

            {(mode === 'login' || mode === 'signup') && (
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
            )}

            {mode === 'login' && (
              <p className="text-zinc-600 text-xs text-center mt-3">
                <button
                  onClick={() => switchMode('forgot')}
                  className="text-zinc-500 hover:text-zinc-300 transition-colors"
                >
                  {t('auth.forgotPassword')}
                </button>
              </p>
            )}
          </div>
        </div>}
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

// Full-page navigation to the backend's OAuth start endpoint, which 302-redirects
// to the provider. The provider then returns to /oauth/callback.
function SocialButton({ provider, label }) {
  const Icon = provider === 'github' ? GithubIcon : GoogleIcon
  return (
    <a
      href={`/auth/oauth/${provider}/start`}
      className="flex items-center justify-center gap-2.5 w-full bg-zinc-900 border border-zinc-800 rounded px-4 py-2.5 text-sm text-zinc-200 hover:bg-zinc-800 hover:border-zinc-700 transition-colors"
    >
      <Icon />
      {label}
    </a>
  )
}

function GithubIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0016 8c0-4.42-3.58-8-8-8z" />
    </svg>
  )
}

function GoogleIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 18 18" aria-hidden="true">
      <path fill="#4285F4" d="M17.64 9.2c0-.64-.06-1.25-.16-1.84H9v3.48h4.84a4.14 4.14 0 0 1-1.8 2.72v2.26h2.91c1.7-1.57 2.69-3.88 2.69-6.62z" />
      <path fill="#34A853" d="M9 18c2.43 0 4.47-.8 5.96-2.18l-2.91-2.26c-.81.54-1.84.86-3.05.86-2.35 0-4.34-1.59-5.05-3.71H.96v2.33A9 9 0 0 0 9 18z" />
      <path fill="#FBBC05" d="M3.95 10.71a5.41 5.41 0 0 1 0-3.42V4.96H.96a9 9 0 0 0 0 8.08l2.99-2.33z" />
      <path fill="#EA4335" d="M9 3.58c1.32 0 2.51.45 3.44 1.35l2.58-2.58A9 9 0 0 0 .96 4.96l2.99 2.33C4.66 5.17 6.65 3.58 9 3.58z" />
    </svg>
  )
}
