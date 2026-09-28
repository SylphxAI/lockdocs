# lockdocs brand

This folder is the source of truth for the lockdocs mark, its small sizes, its
app icon, its colours and its type. Every surface copies from it — `brand.json`
lists which file each surface is a copy of — and nothing draws its own.
`build.py` rebuilds every derived file from the masters in `svg/`:

```bash
pip install pillow numpy resvg-py
python3 brand/build.py            # regenerate (favicon, app icons, tokens.css, provenance)
python3 brand/build.py --resnap   # also redraw the 16/32 px grids from the master
python3 brand/build.py --check    # verify hashes and surface copies (CI runs this; stdlib only)
```

## Name

Running text is `lockdocs`: lower case, one word, everywhere — the README, the
docs site, the npm package `@sylphx/lockdocs`, the registry entry
`io.github.SylphxAI/lockdocs`, and the `lockdocs` binary. No document in this
repository writes `Lockdocs` or `LockDocs`. The only upper-case form is the
`LOCKDOCS_*` environment-variable prefix, which is not the name.

lockdocs is operated by Sylphx Limited. The operator line is never part of the
brand: it belongs to the licence, legal pages and the site footer, and the mark
never carries it.

There are no local-script names: the product ships in English.

The mark is a padlock with no lettering and there is no wordmark, so the name
never appears inside the logo and has no capitalised form anywhere.

## Files

| Need | File |
|---|---|
| The mark, anywhere it is placed (site header, README, slides) | `svg/lockdocs-symbol.svg` |
| One-colour printing, engraving, embroidery, stamps | `svg/lockdocs-symbol-black.svg`, `svg/lockdocs-symbol-white.svg` |
| App icon master (store listing, launcher) | `svg/lockdocs-app-icon.svg` |
| Full-bleed square for masks (Android maskable, Apple touch, vendors that crop) | `svg/lockdocs-maskable.svg` |
| Browser tab | `favicon/favicon.svg` (32 px pixel grid), `favicon/favicon.ico` (16, 32, 48), `favicon/favicon-16.png`, `favicon/favicon-32.png`, `favicon/favicon-48.png` |
| Home screen, store listing, future web-app manifest | `app-icon/icon-192.png`, `app-icon/icon-512.png`, `app-icon/icon-1024.png`, `app-icon/icon-maskable-192.png`, `app-icon/icon-maskable-512.png`, `app-icon/apple-touch-icon-180.png` |
| The website's own logo file (served at `/lockdocs/logo.svg`) | `docs/public/logo.svg` |
| Colour and type tokens | `tokens.json` (source), `tokens.css` (generated) |
| Where every file came from, with its SHA-256 | `provenance.json` |

## Colours

| Token | Hex | Used for |
|---|---|---|
| `--brand-color-bg` | `#06080C` | Page ground; the app-icon ground; the document lines cut out of the lock body |
| `--brand-color-bg-raised` | `#0B0E14` | Alternating background: code blocks, sidebar, footer |
| `--brand-color-bg-elevated` | `#0E1118` | Elevated surfaces: menus, dialogs, hover cards |
| `--brand-color-bg-soft` | `#11151D` | Soft fills and panels |
| `--brand-color-accent` | `#7C9CFF` | The lock body; links, badges and primary buttons |
| `--brand-color-accent-hover` | `#9DB4FF` | Links, badges and primary buttons on hover |
| `--brand-color-accent-active` | `#6480E8` | Links and buttons when pressed |
| `--brand-color-mint` | `#42D6A4` | The shackle; second stop of the hero gradient |
| `--brand-color-gold` | `#FFD166` | Third stop of the hero gradient |

The docs site gets its theme colours from these tokens: `custom.css` imports
`brand/tokens.css` and assigns `--vp-c-*` from `var(--brand-color-*)`, and
`docs/.vitepress/config.ts` reads the `theme-color` meta tag from
`tokens.json`. The values are unchanged from the site that shipped before the
brand home existed.

## Type

Body and interface are set in Inter (`--brand-font-sans`), served by VitePress
with the docs theme from the built site's own `/assets` — self-hosted, no font
CDN — under the SIL Open Font License 1.1. The theme already defaults
`--vp-font-family-base` to it, so no surface declares its own face. No type
face is part of the mark, because the mark has no lettering.

## Small sizes

16 px and 32 px are not scaled renders. `build.py` renders the app icon at 8x,
quantises every sample to the three colours in `brand.json` (`palette`), and
writes the result as a character grid: `favicon/grid-16.txt` and
`favicon/grid-32.txt`. A pixel takes the colour most of its samples agree on,
and stays empty when fewer than `snap_threshold` (0.5) of its samples are
filled.

The grid files are meant to be hand-edited — a later `build.py` run draws from
the grid as it stands; only `--resnap` redraws it from the master.
`favicon/favicon.svg` is the 32 px grid as one path per colour, and
`favicon/favicon.ico` holds 16 and 32 from the grids plus 48 from a direct
render. `palette` must list exactly the colours the master uses, or the grids
pick up colours that are not in the brand.

## Clear space and minimum size

Not yet specified. This repository has no design document, so no clear space or
minimum size has been recorded. The symbol's viewBox is tight to the artwork:
add clearance where the mark is placed, not inside the file.

## Do / Don't

Not yet specified beyond what the files themselves fix: the mark is never
recoloured, so the three accent colours above are the only ones (the one-colour
files are the exception, for print); and the two document lines stay visible in
every version, which in the one-colour files means they are cut out of the lock
body rather than painted over it. The rounded square belongs to the app icon
alone — for anything that crops or masks, use the full-bleed `lockdocs-maskable`.

## Surfaces

| Surface | Is a copy of |
|---|---|
| `docs/public/logo.svg` (site header) | `svg/lockdocs-symbol.svg` |
| `docs/public/favicon.svg` | `favicon/favicon.svg` |
| `docs/public/favicon.ico` | `favicon/favicon.ico` |

`python3 brand/build.py --check` fails if any of these stops being a
byte-for-byte copy. Rebuilding the site is not needed for icon changes: the
files are served from `docs/public/` as they are.

### Surfaces still to move

- `docs/.vitepress/theme/custom.css` keeps four brand tints as literal
  `rgba()`: `--vp-c-brand-soft` (`rgba(124, 156, 255, 0.14)`), the hero
  radial gradient (`rgba(124,156,255,.35)`, `rgba(66,214,164,.2)`) and the
  hero-shot border and shadow (`rgba(124,156,255,.08)`). They are alpha tints
  of `--brand-color-accent` and `--brand-color-mint`; writing them as tokens
  needs `color-mix()`, which drops the whole declaration on browsers older
  than 2023, so they stay literals.
- `README.md` badges: the shields.io URLs carry `color=7c9cff` (npm badge) and
  `42d6a4` (MCP Registry badge). shields.io renders from its own servers and
  cannot read a local token.
- Social preview: `docs/.vitepress/config.ts` sets `og:image` to
  `docs/public/img/demo.gif`, a 1400×780 terminal recording, not a 1200×630
  still. There is no `og/` master. The audit of 2026-09-28 recorded this as a
  gap to fill later rather than draw one now.

## Provenance

- `svg/lockdocs-symbol.svg` is `docs/public/logo.svg` moved here; git history
  for that path starts at `d4c4d54` (2026-09-25, "feat: lockdocs 0.1 —
  exact-version library docs from your lockfile, offline (#1)") and the drawing
  has not changed since. The website file is now generated back as a copy.
- `svg/lockdocs-symbol-black.svg` and `-white.svg` are that drawing in one
  colour: every fill and stroke set to `#000000` / `#FFFFFF`, with the two
  document lines cut out of the lock body by `fill-rule="evenodd"` so they stay
  visible on a black or white ground.
- `svg/lockdocs-app-icon.svg` was specified by the brand lead in the audit of
  2026-09-28: the symbol on a `#06080C` rounded square (`rx` 22% of the side),
  1024 viewBox, symbol at 70% width, centred. `svg/lockdocs-maskable.svg` is the
  same construction without the corner radius, so Apple's and Android's masks
  have a full-bleed square to work with.
- `tokens.json` records the colours and the type face the product already
  shipped: the logo's three colours, the docs theme's background and accent
  roles, and the theme's Inter stack.
- Every file's SHA-256 is in `provenance.json`; the entry for each file says
  where it came from, and `build.py` refreshes the hashes.

## Trademark

Not registered. Owner decision owner#781: no trademark filings before the
product earns money. Use ™ at most, never ®.

<!-- similarity: filled in by review -->
