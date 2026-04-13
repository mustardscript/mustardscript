interface PlaygroundEditorProps {
  title: string
  subtitle: string
  filename: string
  value: string
  onChange: (value: string) => void
}

export function PlaygroundEditor({
  title,
  subtitle,
  filename,
  value,
  onChange,
}: PlaygroundEditorProps) {
  return (
    <section className="playground-panel">
      <div className="flex items-start justify-between gap-4 border-b border-black/10 px-5 py-4">
        <div>
          <h3 className="font-heading text-xl font-bold text-black">{title}</h3>
          <p className="mt-1 text-sm text-black/55">{subtitle}</p>
        </div>
        <span className="rounded-full border border-black/10 bg-black/5 px-3 py-1 font-mono text-xs text-black/60">
          {filename}
        </span>
      </div>
      <textarea
        aria-label={title}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        spellCheck={false}
        className="playground-editor min-h-[21rem] w-full resize-y border-0 bg-transparent p-5 font-mono text-sm leading-7 text-[#F8F2DE] outline-none"
      />
    </section>
  )
}
