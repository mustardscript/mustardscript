// Proper left-to-right tokenizer — never re-matches inside already-highlighted spans
// Warm color palette to match the mustard theme

const KEYWORDS = new Set([
  'const','let','var','await','new','if','else','return','async','function',
  'import','from','export','throw',
])
const TYPES = new Set([
  'Mustard','Progress','MustardExecutor','InMemoryMustardExecutorStore',
  'MustardLimitError','AbortSignal',
])

// Warm palette — amber/earth tones instead of cold blue/violet
const COLORS = {
  comment:  '#7C6F5B',  // warm gray-brown
  string:   '#FBBF24',  // bright amber
  keyword:  '#D4A056',  // warm gold
  type:     '#F5D563',  // light mustard
  call:     '#E8C97A',  // soft warm gold
  property: '#A8C686',  // warm sage green
  number:   '#E09F5C',  // warm orange
  default:  '#D4C8A8',  // warm light tan
}

interface Token { text: string; color?: string; bold?: boolean; italic?: boolean }

function tokenizeLine(line: string): Token[] {
  const tokens: Token[] = []
  let i = 0

  while (i < line.length) {
    // Comment
    if (line[i] === '/' && line[i + 1] === '/') {
      tokens.push({ text: line.slice(i), color: COLORS.comment, italic: true })
      break
    }
    // Block comment
    if (line[i] === '/' && line[i + 1] === '*') {
      const end = line.indexOf('*/', i + 2)
      const j = end >= 0 ? end + 2 : line.length
      tokens.push({ text: line.slice(i, j), color: COLORS.comment, italic: true })
      i = j
      continue
    }

    // String
    if (line[i] === '"' || line[i] === "'" || line[i] === '`') {
      const quote = line[i]
      let j = i + 1
      while (j < line.length && line[j] !== quote) {
        if (line[j] === '\\') j++
        j++
      }
      j++
      tokens.push({ text: line.slice(i, j), color: COLORS.string })
      i = j
      continue
    }

    // Word
    if (/[a-zA-Z_$]/.test(line[i])) {
      let j = i
      while (j < line.length && /[a-zA-Z0-9_$]/.test(line[j])) j++
      const word = line.slice(i, j)

      if (KEYWORDS.has(word)) {
        tokens.push({ text: word, color: COLORS.keyword, bold: true })
      } else if (TYPES.has(word)) {
        tokens.push({ text: word, color: COLORS.type, bold: true })
      } else {
        const rest = line.slice(j)
        const isCall = /^\s*\(/.test(rest)
        const prevChar = i > 0 ? line[i - 1] : ''
        if (isCall) {
          tokens.push({ text: word, color: COLORS.call })
        } else if (prevChar === '.') {
          tokens.push({ text: word, color: COLORS.property })
        } else {
          tokens.push({ text: word, color: COLORS.default })
        }
      }
      i = j
      continue
    }

    // Number
    if (/[0-9]/.test(line[i])) {
      let j = i
      while (j < line.length && /[0-9_]/.test(line[j])) j++
      tokens.push({ text: line.slice(i, j), color: COLORS.number })
      i = j
      continue
    }

    // Any other char
    tokens.push({ text: line[i], color: COLORS.default })
    i++
  }

  return tokens
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

export function highlightLine(line: string): string {
  if (!line.trim()) return '&nbsp;'
  return tokenizeLine(line).map(t => {
    const escaped = escapeHtml(t.text)
    if (!t.color) return escaped
    let style = `color:${t.color}`
    if (t.bold) style += ';font-weight:600'
    if (t.italic) style += ';font-style:italic'
    return `<span style="${style}">${escaped}</span>`
  }).join('')
}

export function highlightCode(code: string, language?: string): string {
  const lang = language?.toLowerCase()
  if (lang === 'json') return highlightJson(code)
  if (lang === 'sh' || lang === 'bash' || lang === 'shell') return highlightShell(code)
  if (lang && lang !== 'js' && lang !== 'ts' && lang !== 'javascript' && lang !== 'typescript') {
    // Unknown language — just escape
    return code.split('\n').map(line => line.trim() ? escapeHtml(line) : '&nbsp;').join('\n')
  }
  return code.split('\n').map(line => highlightLine(line)).join('\n')
}

// JSON tokenizer — warm palette
function highlightJson(code: string): string {
  return code.split('\n').map(line => {
    if (!line.trim()) return '&nbsp;'
    const tokens: string[] = []
    let i = 0
    while (i < line.length) {
      // String (could be key or value)
      if (line[i] === '"') {
        let j = i + 1
        while (j < line.length && line[j] !== '"') {
          if (line[j] === '\\') j++
          j++
        }
        j++
        const str = line.slice(i, j)
        // If followed by ':', it's a key
        const rest = line.slice(j)
        const isKey = /^\s*:/.test(rest)
        tokens.push(`<span style="color:${isKey ? COLORS.property : COLORS.string}">${escapeHtml(str)}</span>`)
        i = j
        continue
      }
      // Number
      if (/[-0-9]/.test(line[i]) && (i === 0 || /[\s,:\[]/.test(line[i - 1]))) {
        let j = i
        if (line[j] === '-') j++
        while (j < line.length && /[0-9.eE+-]/.test(line[j])) j++
        tokens.push(`<span style="color:${COLORS.number}">${escapeHtml(line.slice(i, j))}</span>`)
        i = j
        continue
      }
      // Boolean / null
      const boolMatch = line.slice(i).match(/^(true|false|null)\b/)
      if (boolMatch) {
        tokens.push(`<span style="color:${COLORS.keyword};font-weight:600">${boolMatch[1]}</span>`)
        i += boolMatch[1].length
        continue
      }
      tokens.push(`<span style="color:${COLORS.default}">${escapeHtml(line[i])}</span>`)
      i++
    }
    return tokens.join('')
  }).join('\n')
}

// Shell tokenizer — just comments
function highlightShell(code: string): string {
  return code.split('\n').map(line => {
    if (!line.trim()) return '&nbsp;'
    const trimmed = line.trimStart()
    if (trimmed.startsWith('#')) {
      const indent = escapeHtml(line.slice(0, line.length - trimmed.length))
      return `${indent}<span style="color:${COLORS.comment};font-style:italic">${escapeHtml(trimmed)}</span>`
    }
    return escapeHtml(line)
  }).join('\n')
}
