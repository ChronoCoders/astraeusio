import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { Routes, Route } from 'react-router-dom'
import App from './App.jsx'
import AuthPage from './AuthPage.jsx'
import LandingPage from './components/LandingPage.jsx'
import ProductsPage from './components/ProductsPage.jsx'

export default function Root() {
  const [token,    setToken]    = useState(() => localStorage.getItem('token'))
  const [user,     setUser]     = useState(null)   // null while loading, then { email, plan }
  const [booting,  setBooting]  = useState(false)
  const [authMode, setAuthMode] = useState(null)   // null | 'login' | 'signup'

  // Fetch /api/user/me whenever token is set (login or page reload with stored token).
  useEffect(() => {
    if (!token) return
    let cancelled = false
    fetch('/api/user/me', {
      headers: { Authorization: `Bearer ${token}` },
    })
      .then(r => r.ok ? r.json() : null)
      .then(d => { if (!cancelled) setUser(d ?? { email: '', plan: 'starter' }) })
      .catch(() => { if (!cancelled) setUser({ email: '', plan: 'starter' }) })
    return () => { cancelled = true }
  }, [token])

  function handleAuth(t) {
    localStorage.setItem('token', t)
    setUser(null)   // reset so effect re-fetches for new token
    setToken(t)
    setBooting(true)
    setAuthMode(null)
  }

  function handleLogout() {
    localStorage.removeItem('token')
    setToken(null)
    setUser(null)
    setBooting(false)
  }

  if (!token) {
    if (authMode) return <AuthPage onAuth={handleAuth} initialMode={authMode} />
    const landingProps = { onSignUp: () => setAuthMode('signup'), onSignIn: () => setAuthMode('login') }
    return (
      <Routes>
        <Route path="/"         element={<LandingPage  {...landingProps} />} />
        <Route path="/products" element={<ProductsPage {...landingProps} />} />
        <Route path="*"         element={<LandingPage  {...landingProps} />} />
      </Routes>
    )
  }

  // Show loader while we still have no user object (plan fetch in-flight)
  // or while App itself is booting.
  const showLoader = booting || user === null

  return (
    <>
      {showLoader && <DashboardLoader />}
      <div style={showLoader ? { display: 'none' } : undefined}>
        <App user={user} onLogout={handleLogout} onReady={() => setBooting(false)} />
      </div>
    </>
  )
}

function DashboardLoader() {
  const { t } = useTranslation()
  return (
    <div className="min-h-screen flex items-center justify-center bg-zinc-950">
      <div className="flex flex-col items-center gap-3">
        <svg className="animate-spin h-5 w-5 text-zinc-400" fill="none" viewBox="0 0 24 24">
          <circle className="opacity-25" cx="12" cy="12" r="10"
            stroke="currentColor" strokeWidth="4" />
          <path className="opacity-75" fill="currentColor"
            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962
               7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
        </svg>
        <p className="text-zinc-500 text-xs font-mono tracking-wide">
          {t('auth.loadingDashboard')}
        </p>
      </div>
    </div>
  )
}
