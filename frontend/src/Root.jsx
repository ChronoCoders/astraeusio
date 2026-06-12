import { lazy, Suspense, useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { Routes, Route, useLocation } from 'react-router-dom'

// Lazy route splits: each page becomes its own chunk so visitors only download
// the code for the route they actually land on.
const App              = lazy(() => import('./App.jsx'))
const AuthPage         = lazy(() => import('./AuthPage.jsx'))
const LandingPage      = lazy(() => import('./components/LandingPage.jsx'))
const ProductsPage     = lazy(() => import('./components/ProductsPage.jsx'))
const PricingPage      = lazy(() => import('./components/PricingPage.jsx'))
const DocsPage         = lazy(() => import('./components/DocsPage.jsx'))
const AboutPage        = lazy(() => import('./components/AboutPage.jsx'))
const BlogPage         = lazy(() => import('./components/BlogPage.jsx'))
const BlogPostPage     = lazy(() => import('./components/BlogPostPage.jsx'))
const VerifyEmailPage  = lazy(() => import('./components/VerifyEmailPage.jsx'))
const StatusPage       = lazy(() => import('./components/StatusPage.jsx'))
const PrivacyPage      = lazy(() => import('./components/PrivacyPage.jsx'))
const TermsPage        = lazy(() => import('./components/TermsPage.jsx'))
const NotFoundPage     = lazy(() => import('./components/NotFoundPage.jsx'))

export default function Root() {
  const [token,    setToken]    = useState(() => localStorage.getItem('token'))
  const [user,     setUser]     = useState(null)   // null while loading, then { email, plan }
  const [booting,  setBooting]  = useState(false)
  const [authMode, setAuthMode] = useState(null)   // null | 'login' | 'signup'
  const location = useLocation()

  // Fetch /api/user/me whenever token is set (login or page reload with stored token).
  // On 401, clear the stale/expired token so the user is returned to the login page.
  useEffect(() => {
    if (!token) return
    let cancelled = false
    const ctrl = new AbortController()
    const timeout = setTimeout(() => ctrl.abort(), 5000)
    fetch('/api/user/me', {
      headers: { Authorization: `Bearer ${token}` },
      signal: ctrl.signal,
    })
      .then(r => {
        if (r.status === 401) {
          if (!cancelled) clearSession()
          return null
        }
        return r.ok ? r.json() : null
      })
      .then(d => { if (!cancelled && d !== null) setUser(d ?? { email: '', plan: 'starter' }) })
      .catch(() => { if (!cancelled) setUser({ email: '', plan: 'starter' }) })
      .finally(() => clearTimeout(timeout))
    return () => { cancelled = true; ctrl.abort() }
  }, [token])

  // Re-fetch user when the tab regains focus (e.g. after clicking a verification email link).
  useEffect(() => {
    if (!token) return
    const onVisible = () => {
      if (document.visibilityState !== 'visible') return
      fetch('/api/user/me', { headers: { Authorization: `Bearer ${token}` } })
        .then(r => r.ok ? r.json() : null)
        .then(d => { if (d) setUser(d) })
        .catch(() => {})
    }
    document.addEventListener('visibilitychange', onVisible)
    return () => document.removeEventListener('visibilitychange', onVisible)
  }, [token])

  function clearSession() {
    localStorage.removeItem('token')
    setToken(null)
    setUser(null)
    setBooting(false)
  }

  function handleAuth(t) {
    localStorage.setItem('token', t)
    setUser(null)   // reset so effect re-fetches for new token
    setToken(t)
    setBooting(true)
    setAuthMode(null)
  }

  function handleLogout() {
    clearSession()
    setAuthMode('login')
  }

  if (!token) {
    if (authMode) return (
      <Suspense fallback={<RouteLoader />}>
        <AuthPage onAuth={handleAuth} initialMode={authMode} />
      </Suspense>
    )
    const pub = { onSignUp: () => setAuthMode('signup'), onSignIn: () => setAuthMode('login') }
    return (
      <Suspense fallback={<RouteLoader />}>
        <Routes>
          <Route path="/"         element={<LandingPage  {...pub} />} />
          <Route path="/products" element={<ProductsPage {...pub} />} />
          <Route path="/pricing"  element={<PricingPage  {...pub} />} />
          <Route path="/docs"     element={<DocsPage     {...pub} />} />
          <Route path="/about"    element={<AboutPage    {...pub} />} />
          <Route path="/blog"           element={<BlogPage       {...pub} />} />
          <Route path="/blog/:slug"     element={<BlogPostPage   {...pub} />} />
          <Route path="/verify-email"   element={<VerifyEmailPage {...pub} onUserChange={setUser} />} />
          <Route path="/status"         element={<StatusPage      {...pub} />} />
          <Route path="/privacy"        element={<PrivacyPage     {...pub} />} />
          <Route path="/terms"          element={<TermsPage       {...pub} />} />
          <Route path="/reset-password" element={<AuthPage initialMode="reset" onAuth={handleAuth} />} />
          <Route path="/oauth/callback" element={<OAuthCallback onAuth={handleAuth} />} />
          <Route path="*"               element={<NotFoundPage   {...pub} />} />
        </Routes>
      </Suspense>
    )
  }

  // Logged-in user navigating to /verify-email (clicked link while already signed in)
  if (location.pathname === '/verify-email') {
    return (
      <Suspense fallback={<RouteLoader />}>
        <Routes>
          <Route path="/verify-email" element={
            <VerifyEmailPage onSignIn={() => {}} onSignUp={() => {}} onUserChange={setUser} token={token} />
          } />
        </Routes>
      </Suspense>
    )
  }

  // Show loader while we still have no user object (plan fetch in-flight)
  // or while App itself is booting.
  const showLoader = booting || user === null

  return (
    <>
      {showLoader && <DashboardLoader />}
      <div style={showLoader ? { display: 'none' } : undefined}>
        <Suspense fallback={<DashboardLoader />}>
          <App user={user} onLogout={handleLogout} onReady={() => setBooting(false)} onUserChange={setUser} />
        </Suspense>
      </div>
    </>
  )
}

// Lands here after a provider redirect. The backend places the result in the URL
// fragment (never sent to servers): `#token=…` (signed in), `#partial_token=…`
// (account has 2FA - finish with TOTP), or `#error=code`.
function OAuthCallback({ onAuth }) {
  const [parsed] = useState(() => {
    const p = new URLSearchParams(window.location.hash.replace(/^#/, ''))
    return { token: p.get('token'), partial: p.get('partial_token'), error: p.get('error') }
  })

  useEffect(() => {
    if (parsed.token) {
      // Strip the token from the address bar before transitioning to the app.
      window.history.replaceState(null, '', '/')
      onAuth(parsed.token)
    }
  }, [parsed.token, onAuth])

  if (parsed.token) return <DashboardLoader />
  if (parsed.partial) {
    return <AuthPage initialMode="login" initialPartialToken={parsed.partial} onAuth={onAuth} />
  }
  return <AuthPage initialMode="login" oauthErrorCode={parsed.error ?? 'oauth_failed'} onAuth={onAuth} />
}

function Spinner() {
  return (
    <svg className="animate-spin h-5 w-5 text-zinc-400" fill="none" viewBox="0 0 24 24">
      <circle className="opacity-25" cx="12" cy="12" r="10"
        stroke="currentColor" strokeWidth="4" />
      <path className="opacity-75" fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962
           7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
    </svg>
  )
}

function RouteLoader() {
  return (
    <div className="min-h-screen flex items-center justify-center bg-zinc-950">
      <Spinner />
    </div>
  )
}

function DashboardLoader() {
  const { t } = useTranslation()
  return (
    <div className="min-h-screen flex items-center justify-center bg-zinc-950">
      <div className="flex flex-col items-center gap-3">
        <Spinner />
        <p className="text-zinc-500 text-xs font-mono tracking-wide">
          {t('auth.loadingDashboard')}
        </p>
      </div>
    </div>
  )
}
