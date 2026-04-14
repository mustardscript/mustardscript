#!/usr/bin/env node
import { createServer } from 'node:http';
import { readdir, readFile } from 'node:fs/promises';
import { extname, join, resolve } from 'node:path';

const ROOT = resolve(new URL('..', import.meta.url).pathname);
const PORT = Number(process.env.PORT) || 5178;

const SECTIONS = [
  {
    id: 'originals',
    title: 'Originals — 12 directions',
    subtitle: 'Each is a different concept for a lowercase m in mustard.',
    dir: join(ROOT, 'ux-artifacts', 'logos'),
    labels: {
      '01-minimalist-flat.png': 'Minimalist flat',
      '02-mustard-bottle.png': 'Mustard bottle',
      '03-squeezed-mustard.png': 'Squeezed mustard',
      '04-gradient-mono.png': 'Gradient mono',
      '05-sticker-outline.png': 'Sticker outline',
      '06-3d-glossy.png': '3D glossy',
      '07-mustard-seeds.png': 'Mustard seeds',
      '08-pixel-art.png': 'Pixel art',
      '09-mascot-character.png': 'Mascot character',
      '10-wax-seal.png': 'Wax seal',
      '11-neon-sign.png': 'Neon sign',
      '12-hot-dog-squeeze.png': 'Hot dog squeeze',
    },
  },
  {
    id: 'squeezed',
    title: 'Squeezed-mustard riffs — 24 variations',
    subtitle: 'All riff on 03 (squeezed mustard) across medium, surface, and style.',
    dir: join(ROOT, 'ux-artifacts', 'logos-squeezed'),
    labels: {
      '01-clean-white-plate.png': 'Clean white plate',
      '02-cutting-board.png': 'Cutting board',
      '03-mid-squeeze-action.png': 'Mid-squeeze action',
      '04-dijon-grainy.png': 'Dijon grainy',
      '05-soft-pretzel.png': 'Soft pretzel',
      '06-hot-dog.png': 'Hot dog',
      '07-honey-mustard-thin.png': 'Honey mustard thin',
      '08-dark-slate.png': 'Dark slate',
      '09-flat-vector.png': 'Flat vector',
      '10-ink-sketch.png': 'Ink sketch',
      '11-watercolor.png': 'Watercolor',
      '12-chalkboard.png': 'Chalkboard',
      '13-line-art.png': 'Line art',
      '14-retro-diner.png': 'Retro diner',
      '15-pop-art.png': 'Pop art',
      '16-risograph.png': 'Risograph',
      '17-isometric-3d.png': 'Isometric 3D',
      '18-low-poly.png': 'Low-poly',
      '19-enamel-pin.png': 'Enamel pin',
      '20-embroidered-patch.png': 'Embroidered patch',
      '21-brush-calligraphy.png': 'Brush calligraphy',
      '22-graffiti-tag.png': 'Graffiti tag',
      '23-rubber-stamp.png': 'Rubber stamp',
      '24-neon-sign.png': 'Neon sign',
    },
  },
];

const DIR_BY_ID = Object.fromEntries(SECTIONS.map((s) => [s.id, s.dir]));

async function listImages(dir) {
  const entries = await readdir(dir).catch(() => []);
  return entries.filter((f) => f.toLowerCase().endsWith('.png')).sort();
}

function cardsHtml(section, images) {
  return images
    .map((name, i) => {
      const label = section.labels[name] ?? name.replace(/\.png$/, '');
      return `<figure class="card">
        <div class="frame"><img src="/img/${section.id}/${encodeURIComponent(name)}" alt="${label}" loading="lazy" /></div>
        <figcaption><span class="idx">${String(i + 1).padStart(2, '0')}</span><span class="name">${label}</span></figcaption>
      </figure>`;
    })
    .join('\n');
}

async function page() {
  const sectionsHtml = (
    await Promise.all(
      SECTIONS.map(async (s) => {
        const imgs = await listImages(s.dir);
        return `<section id="${s.id}">
          <header class="section-head">
            <h2>${s.title}</h2>
            <p>${s.subtitle}</p>
            <span class="count">${imgs.length} ${imgs.length === 1 ? 'variant' : 'variants'}</span>
          </header>
          ${imgs.length
            ? `<div class="grid">${cardsHtml(s, imgs)}</div>`
            : `<div class="empty">No images yet in ${s.dir.replace(ROOT + '/', '')}.</div>`}
        </section>`;
      }),
    )
  ).join('\n');

  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>MustardScript — Logo Explorations</title>
<link rel="preconnect" href="https://fonts.googleapis.com" />
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
<link href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@500;600;700&family=DM+Sans:wght@400;500;600&family=JetBrains+Mono:wght@500&display=swap" rel="stylesheet" />
<style>
  :root {
    --mustard: #E8B931; --mustard-dark: #C99A1D; --dijon: #C49102; --deep: #A67C17;
    --honey: #FFF8E1; --bg: #FFFDF7; --card: #FEF3C7; --card-hover: #FDE68A;
    --text: #1C1917; --text-dim: #78716C; --border: rgba(0,0,0,0.08);
    --glow: rgba(232, 185, 49, 0.35);
  }
  * { box-sizing: border-box; }
  body { margin: 0; font-family: 'DM Sans', system-ui, sans-serif; background: var(--bg); color: var(--text); -webkit-font-smoothing: antialiased; }
  header.top {
    padding: 64px 32px 24px; text-align: center;
    background: linear-gradient(180deg, var(--honey), var(--bg));
    border-bottom: 1px solid var(--border);
  }
  header.top h1 {
    font-family: 'Space Grotesk', sans-serif; font-weight: 700;
    font-size: clamp(2rem, 5vw, 3.5rem); margin: 0 0 12px; letter-spacing: -0.02em;
  }
  header.top h1 .m {
    background: linear-gradient(135deg, #B45309, #A16207, #854D0E);
    -webkit-background-clip: text; -webkit-text-fill-color: transparent; background-clip: text;
  }
  header.top .sub { color: var(--text-dim); font-size: 1.05rem; margin: 0; }
  header.top .sub code { font-family: 'JetBrains Mono', monospace; }
  header.top nav { margin-top: 18px; display: flex; justify-content: center; gap: 8px; flex-wrap: wrap; }
  header.top nav a {
    padding: 6px 14px; border-radius: 999px; background: var(--mustard);
    color: #1C1917; text-decoration: none; font-weight: 600; font-size: 0.85rem;
    transition: transform 0.15s ease;
  }
  header.top nav a:hover { transform: translateY(-1px); }
  main { max-width: 1400px; margin: 0 auto; padding: 48px 24px 96px; }
  section { margin-bottom: 64px; scroll-margin-top: 24px; }
  .section-head {
    display: flex; align-items: baseline; gap: 16px; flex-wrap: wrap;
    margin-bottom: 24px; padding-bottom: 12px; border-bottom: 1px solid var(--border);
  }
  .section-head h2 {
    font-family: 'Space Grotesk', sans-serif; font-size: 1.75rem; margin: 0;
  }
  .section-head p { color: var(--text-dim); margin: 0; flex: 1 1 200px; font-size: 0.95rem; }
  .count {
    padding: 4px 12px; border-radius: 999px; background: var(--mustard); color: #1C1917;
    font-family: 'JetBrains Mono', monospace; font-size: 0.8rem; font-weight: 600;
    box-shadow: 0 4px 20px var(--glow);
  }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 20px; }
  .card {
    margin: 0; background: var(--card); border: 1px solid var(--border);
    border-radius: 16px; overflow: hidden;
    transition: transform 0.2s ease, box-shadow 0.2s ease, background 0.2s ease;
  }
  .card:hover {
    transform: translateY(-4px); background: var(--card-hover);
    box-shadow: 0 12px 40px rgba(180, 83, 9, 0.18);
  }
  .frame { aspect-ratio: 1; background: var(--honey); display: flex; align-items: center; justify-content: center; overflow: hidden; }
  .frame img { width: 100%; height: 100%; object-fit: cover; display: block; }
  figcaption { padding: 12px 14px; display: flex; align-items: center; gap: 8px; font-size: 0.88rem; }
  .idx { font-family: 'JetBrains Mono', monospace; font-size: 0.75rem; color: var(--deep); font-weight: 600; }
  .name { font-weight: 500; }
  .empty { text-align: center; padding: 60px 20px; color: var(--text-dim); font-family: 'JetBrains Mono', monospace; }
  footer { text-align: center; padding: 24px; color: var(--text-dim); font-size: 0.85rem; border-top: 1px solid var(--border); }
</style>
</head>
<body>
<header class="top">
  <h1>Logo explorations for <span class="m">mustardscript</span></h1>
  <p class="sub">Two batches — the original 12 and 24 squeezed-mustard riffs.</p>
  <nav>
    <a href="#originals">Originals (12)</a>
    <a href="#squeezed">Squeezed riffs (24)</a>
  </nav>
</header>
<main>${sectionsHtml}</main>
<footer>Served from <code>ux-artifacts/logos/</code> and <code>ux-artifacts/logos-squeezed/</code></footer>
</body>
</html>`;
}

const MIME = { '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.svg': 'image/svg+xml' };

const server = createServer(async (req, res) => {
  try {
    const url = new URL(req.url, `http://${req.headers.host}`);
    if (url.pathname === '/' || url.pathname === '/index.html') {
      const html = await page();
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' });
      res.end(html);
      return;
    }
    const m = url.pathname.match(/^\/img\/([^/]+)\/(.+)$/);
    if (m) {
      const [, sectionId, rawName] = m;
      const dir = DIR_BY_ID[sectionId];
      if (!dir) { res.writeHead(404); res.end('unknown section'); return; }
      const name = decodeURIComponent(rawName);
      if (name.includes('/') || name.includes('..')) {
        res.writeHead(400); res.end('bad path'); return;
      }
      const data = await readFile(join(dir, name));
      const ct = MIME[extname(name).toLowerCase()] || 'application/octet-stream';
      res.writeHead(200, { 'Content-Type': ct, 'Cache-Control': 'no-store' });
      res.end(data);
      return;
    }
    res.writeHead(404); res.end('not found');
  } catch (err) {
    res.writeHead(500); res.end(String(err));
  }
});

server.listen(PORT, () => {
  console.log(`\n  mustardscript logo gallery\n  → http://localhost:${PORT}\n`);
});
