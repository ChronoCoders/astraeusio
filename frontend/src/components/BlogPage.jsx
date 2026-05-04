import { Link } from 'react-router-dom'
import Navbar from './Navbar'
import { posts } from '../blog/posts'

function TagBadge({ tag }) {
  return (
    <span className="text-[10px] font-mono border border-zinc-700 text-zinc-500 rounded px-1.5 py-0.5">
      {tag}
    </span>
  )
}

export default function BlogPage({ onSignIn }) {
  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">
      <Navbar onSignIn={onSignIn} />

      <div className="max-w-4xl mx-auto px-6 pt-36 pb-24">
        <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase mb-6">Blog</p>
        <h1 className="text-4xl font-thin tracking-tight text-zinc-100 mb-16">
          Space weather, explained.
        </h1>

        <div className="flex flex-col divide-y divide-zinc-800">
          {posts.map(post => (
            <article key={post.slug} className="py-10 first:pt-0">
              <Link to={`/blog/${post.slug}`} className="block mb-5 overflow-hidden rounded-md">
                <img
                  src={post.image}
                  alt={post.title}
                  className="w-full h-44 object-cover opacity-70 hover:opacity-90 hover:scale-105 transition-all duration-300"
                />
              </Link>

              <div className="flex items-center gap-3 mb-4">
                <time className="text-xs font-mono text-zinc-500">{post.date}</time>
                <span className="text-zinc-700">·</span>
                <span className="text-xs font-mono text-zinc-500">{post.author}</span>
              </div>

              <h2 className="text-xl font-light text-zinc-100 mb-3 leading-snug">
                <Link
                  to={`/blog/${post.slug}`}
                  className="hover:text-orange-400 transition-colors"
                >
                  {post.title}
                </Link>
              </h2>

              <p className="text-zinc-400 text-sm leading-relaxed mb-5 max-w-2xl">
                {post.excerpt}
              </p>

              <div className="flex items-center gap-4">
                <div className="flex gap-2 flex-wrap">
                  {post.tags.map(tag => <TagBadge key={tag} tag={tag} />)}
                </div>
                <Link
                  to={`/blog/${post.slug}`}
                  className="text-xs font-mono text-zinc-500 hover:text-zinc-200 transition-colors ml-auto shrink-0"
                >
                  Read more →
                </Link>
              </div>
            </article>
          ))}
        </div>
      </div>
    </div>
  )
}
