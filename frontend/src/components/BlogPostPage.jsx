import { Link, useParams, Navigate } from 'react-router-dom'
import Navbar from './Navbar'
import { getPost } from '../blog/posts'

function TagBadge({ tag }) {
  return (
    <span className="text-[10px] font-mono border border-zinc-700 text-zinc-500 rounded px-1.5 py-0.5">
      {tag}
    </span>
  )
}

function renderContent(text) {
  const lines = text.split('\n')
  const elements = []
  let i = 0

  while (i < lines.length) {
    const line = lines[i]

    if (line.startsWith('## ')) {
      elements.push(
        <h2 key={i} className="text-xl font-light text-zinc-100 mt-12 mb-4">
          {line.slice(3)}
        </h2>
      )
      i++
      continue
    }

    // Table — collect all rows
    if (line.startsWith('|')) {
      const rows = []
      while (i < lines.length && lines[i].startsWith('|')) {
        rows.push(lines[i])
        i++
      }
      const headers = rows[0].split('|').filter(Boolean).map(c => c.trim())
      const body = rows.slice(2).map(r => r.split('|').filter(Boolean).map(c => c.trim()))
      elements.push(
        <div key={`table-${i}`} className="overflow-x-auto my-6">
          <table className="w-full text-xs font-mono border border-zinc-800 rounded-md">
            <thead>
              <tr className="border-b border-zinc-800 bg-zinc-900">
                {headers.map((h, j) => (
                  <th key={j} className="text-left px-4 py-2 text-zinc-500 font-normal">{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {body.map((row, ri) => (
                <tr key={ri} className="border-b border-zinc-800/50 last:border-0 hover:bg-zinc-900/50">
                  {row.map((cell, ci) => (
                    <td key={ci} className="px-4 py-2.5 text-zinc-300 align-top">{cell}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )
      continue
    }

    if (line === '') {
      i++
      continue
    }

    // Paragraph — collect consecutive non-special lines
    const paraLines = []
    while (i < lines.length && lines[i] !== '' && !lines[i].startsWith('## ') && !lines[i].startsWith('|')) {
      paraLines.push(lines[i])
      i++
    }
    if (paraLines.length > 0) {
      const raw = paraLines.join(' ')
      // Inline bold (**text**)
      const parts = raw.split(/(\*\*[^*]+\*\*)/)
      elements.push(
        <p key={`p-${i}`} className="text-zinc-400 text-sm leading-relaxed mb-4">
          {parts.map((part, pi) =>
            part.startsWith('**') && part.endsWith('**')
              ? <strong key={pi} className="text-zinc-300 font-medium">{part.slice(2, -2)}</strong>
              : part
          )}
        </p>
      )
    }
  }

  return elements
}

export default function BlogPostPage({ onSignIn }) {
  const { slug } = useParams()
  const post = getPost(slug)

  if (!post) return <Navigate to="/blog" replace />

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">
      <Navbar onSignIn={onSignIn} />

      {/* Hero image */}
      <div className="relative w-full h-56 sm:h-80 mt-16 overflow-hidden">
        <img
          src={post.image}
          alt={post.title}
          className="w-full h-full object-cover opacity-50"
        />
        <div className="absolute inset-0 bg-gradient-to-b from-zinc-950/20 to-zinc-950" />
      </div>

      <article className="max-w-2xl mx-auto px-6 pt-10 pb-24">
        {/* Back */}
        <Link
          to="/blog"
          className="inline-block text-xs font-mono text-zinc-600 hover:text-zinc-400 transition-colors mb-10"
        >
          ← All posts
        </Link>

        {/* Meta */}
        <div className="flex items-center gap-3 mb-6">
          <time className="text-xs font-mono text-zinc-500">{post.date}</time>
          <span className="text-zinc-700">·</span>
          <span className="text-xs font-mono text-zinc-500">{post.author}</span>
        </div>

        {/* Title */}
        <h1 className="text-3xl sm:text-4xl font-thin tracking-tight text-zinc-100 leading-tight mb-6">
          {post.title}
        </h1>

        {/* Tags */}
        <div className="flex gap-2 flex-wrap mb-12">
          {post.tags.map(tag => <TagBadge key={tag} tag={tag} />)}
        </div>

        {/* Content */}
        <div className="border-t border-zinc-800 pt-10">
          {renderContent(post.content)}
        </div>

        {/* Footer */}
        <div className="mt-16 pt-8 border-t border-zinc-800 flex items-center justify-between">
          <Link
            to="/blog"
            className="text-xs font-mono text-zinc-500 hover:text-zinc-200 transition-colors"
          >
            ← All posts
          </Link>
          <span className="text-xs font-mono text-zinc-600">{post.author}</span>
        </div>
      </article>
    </div>
  )
}
