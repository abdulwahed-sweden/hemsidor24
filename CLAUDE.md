# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this repo is

A single-page marketing site for **Hemsidor24** — fixed-price one-page websites for
small Swedish businesses (Start 2 490 kr / Pro 4 490 kr). Bilingual sv/en, with an
order form. Contact address in the copy: `hej@hemsidor24.se`.

The site is being rebuilt as a Rust workspace (`crates/hemsidor24-*`), one
reviewable phase at a time. See README.md for the crate layout and phase status.
Build with `cargo build --workspace`; every phase must pass
`cargo clippy --workspace --all-targets --all-features -- -D warnings`.

`reference/` holds input material only — the original design export. Nothing in
the build reads from it, at runtime or at build time.

**`reference/source.html` is local-only and deliberately untracked** (see
`.gitignore`). It is the ~549 KB Claude Design bundle the public page was
reconstructed from. A clone will not have it, and does not need it: the site
builds and runs without it. You only need it on disk when deliberately
re-running the design extraction below, or comparing the templates against the
original export.

## reference/source.html — read this before editing it

`reference/source.html` is a **self-extracting bundle exported from Claude
Design's canvas editor**, not hand-written HTML. Its structure (line numbers are
for that file):

- Lines 1–~370: the unpacker — a `DOMContentLoaded` script that reads the two
  payload `<script>` tags below, mints blob URLs for each asset, and
  `document.write`s the real page.
- Line 534: `<script type="__bundler/manifest">` — one JSON object, ~481 KB on a
  single line. Keys are UUIDs; values are `{mime, compressed, data}` with
  base64 (gzip'd when `compressed: true`) payloads. Contents:
  - `f6e00033-…` — `dc-runtime` (Claude Design's `<x-dc>` runtime; header says
    "GENERATED from dc-runtime/src/*.ts — do not edit")
  - `5ca59af5-…` / `b3fa80c6-…` — React 18 + ReactDOM production builds
  - six `font/woff2` blobs — Newsreader (400/500/600) and Space Grotesk (500/700)
- Line 546: `<script type="__bundler/template">` — the **actual page source**, a
  JSON-encoded HTML string on a single line. This is what you edit.

Inside the template, asset UUIDs appear as bare `src=`/`url()` values; the unpacker
substitutes blob URLs at runtime. Never rename or renumber a UUID.

### Editing the page

Do **not** try to edit line 546 in place with Edit or `sed` — it is one 45 KB
JSON-escaped line. Unpack, edit, repack:

```bash
# unpack (writes template.html into your scratchpad)
python3 -c "
import json
s = json.load(open('/dev/stdin'))
open('template.html','w',encoding='utf8').write(s)
" <<< "$(sed -n '546p' reference/source.html)"

# ...edit template.html normally...

# repack
python3 - <<'EOF'
import json
lines = open('reference/source.html', encoding='utf8').read().split('\n')
tmpl  = open('template.html',  encoding='utf8').read()
lines[545] = json.dumps(tmpl, ensure_ascii=False).replace('</', '<\\u002F')
open('reference/source.html','w',encoding='utf8').write('\n'.join(lines))
EOF
```

The `.replace('</', '<\\u002F')` is required, not cosmetic: the template contains
literal `</script>` and `</head>` text that would otherwise terminate the enclosing
`<script>` tag. This round-trip is byte-identical on an unmodified file — verify
that before trusting a modified repack.

If regenerating the whole bundle from the Claude Design canvas is an option, prefer
that over hand-repacking.

## Page conventions (inside the template)

The page is a `<x-dc>` component: declarative markup plus one `<script type="text/x-dc">`
block at the end holding a `class Component extends DCLogic`.

- **Interpolation**: `{{ expr }}` in text and in attribute values, resolved against
  the object returned by `renderVals()`.
- **Control flow**: `<sc-for list="{{ t.steps }}" as="s">` and
  `<sc-if value="{{ sent }}">`. The `hint-placeholder-count` / `hint-placeholder-val`
  attributes are canvas-editor hints for rendering an unbound preview — keep them
  roughly in sync with real data, they do not affect runtime.
- **Events / camelCase props**: `sc-camel-on-click`, `sc-camel-on-submit`,
  `sc-camel-no-validate`, `sc-camel-auto-complete` map to React's `onClick`,
  `onSubmit`, `noValidate`, `autoComplete`.
- **Props**: declared as JSON in `data-props` on the script tag (currently just
  `defaultLang`, an enum of `sv`/`en`) and read via `this.props`.
- **Styling is all inline `style=""`.** Two exceptions live in the `<helmet>` block:
  `style-hover="…"` attributes for hover states, and a single `@media (max-width:720px)`
  block that targets `[data-m="…"]` markers (`sec`, `hero`, `card`, `form`, `head`,
  `foot`, `btn`, `actions`, `order`, `sticky`, `mail`). **All responsive behaviour
  goes through `data-m`** — add a marker rather than a class or an id.
- `<helmet>` holds everything destined for `<head>`: `@font-face` rules, global
  resets, keyframes (`rise`, `drift`), and that media query.

### Copy and state

- All user-visible text lives in the `T` object, `T.sv` and `T.en`, at the top of the
  script block. **Every string must be added to both.** Nothing is hardcoded in markup.
- Component state: `{ lang, style, pak, sent }`. Language persists to
  `localStorage['hemsidor24-lang']`; the `defaultLang` prop only applies when nothing
  is stored yet.
- The order form does not submit anywhere — `submit()` runs `:invalid` validation,
  focuses the first bad field, then sets `sent: true` to show a confirmation. If you
  wire up a real backend, that is where it goes.

### Design tokens (used literally throughout — no CSS variables)

Blue `#1C4F9C` (deep `#173F7D`, light `#2560B5`), orange `#E4762A` / `#F2A15C`,
ink `#0E1A2B`, body text `#243247` / `#3B4759` / `#4A5568`,
backgrounds `#F3F5F9` / `#FFFFFF` / `#E9EEF6` / `#EEF2F8`, dark sections `#0E1A2B` / `#0B1730`.
Headings and UI use `'Space Grotesk'`; body copy uses `'Newsreader', Georgia, serif`.

## Content constraints

The site's own copy commits to a deliberately narrow scope ("What we don't do":
no multi-page sites, no more than six services, no web shop, no booking system, no
custom layout or fonts, no meetings). Prices, the "ready tomorrow" promise, and the
"you pay only after you've seen the site" guarantee appear in several places —
change them everywhere, in both languages, or not at all.
