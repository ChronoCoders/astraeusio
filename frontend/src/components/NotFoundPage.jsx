import { Link } from 'react-router-dom'
import Navbar from './Navbar'
import Footer from './Footer'

export default function NotFoundPage({ onSignIn }) {
  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100 flex flex-col">
      <Navbar onSignIn={onSignIn} />

      <div className="flex-1 flex flex-col items-center justify-center px-6 text-center">
        <p className="text-[80px] font-thin text-zinc-800 leading-none select-none tabular-nums">404</p>
        <h1 className="text-2xl font-thin tracking-tight text-zinc-100 mt-4 mb-3">
          Page not found
        </h1>
        <p className="text-zinc-500 text-sm max-w-sm leading-relaxed mb-8">
          This page doesn't exist or has been moved. Check the URL or head back home.
        </p>
        <div className="flex items-center gap-4">
          <Link
            to="/"
            className="px-6 py-2.5 bg-zinc-100 text-zinc-950 text-sm font-mono rounded-lg hover:bg-white transition-colors"
          >
            Go home
          </Link>
          <Link
            to="/status"
            className="px-6 py-2.5 border border-zinc-700 text-zinc-300 text-sm font-mono rounded-lg hover:border-zinc-500 hover:text-zinc-100 transition-colors"
          >
            System status
          </Link>
        </div>
      </div>

      <Footer />
    </div>
  )
}
