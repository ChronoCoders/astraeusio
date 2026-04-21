import { useState } from 'react'
import App from './App.jsx'
import AuthPage from './AuthPage.jsx'

export default function Root() {
  const [token, setToken] = useState(() => localStorage.getItem('token'))

  function handleAuth(t) { setToken(t) }

  function handleLogout() {
    localStorage.removeItem('token')
    setToken(null)
  }

  if (!token) return <AuthPage onAuth={handleAuth} />
  return <App onLogout={handleLogout} />
}
