import { Link } from 'react-router-dom'

export function Footer() {
  return (
    <footer className="py-10 px-6 text-center">
      <p className="text-sm text-black/40">
        Apache-2.0 &middot;{' '}
        <Link
          to="/docs"
          className="text-black/60 hover:text-black transition-colors underline underline-offset-2"
        >
          Docs
        </Link>
        {' '}&middot;{' '}
        <a
          href="https://github.com/mustardscript/mustardscript"
          target="_blank"
          rel="noopener noreferrer"
          className="text-black/60 hover:text-black transition-colors underline underline-offset-2"
        >
          GitHub
        </a>
      </p>
    </footer>
  )
}
