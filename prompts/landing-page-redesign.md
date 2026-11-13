# ConzaCut landing page redesign prompt

Paste everything below the line into a fresh agent session that has this
repository checked out and can run the web app. It changes the landing page
only. It does not deploy.

---

You are redesigning the landing page of **ConzaCut**, the video editor hosted at
https://conzacut.conzahub.com (a fork of OpenCut, MIT). The page today is a
single centred hero on a flat black background. The owner likes its character
and wants it to stay recognisably the same page, but to look far better: a
landing page a visitor remembers and that shows what the editor can actually do.

## What is there now (verify before relying on it)

- Route: `apps/web/src/routes/index.tsx` renders `Header`, `Hero`, `Footer`.
- `apps/web/src/components/landing/hero.tsx`: the headline "The open source /
  Video editor", a one-line description, and one button that links to
  `/projects`.
- `apps/web/src/components/landing/handlebars.tsx`: the yellow trim handles
  around "Video editor". They are draggable and mask the text like trimming a
  clip. This is the page's signature. Keep it and build the design around it.
- The hero points at `/landing-page-dark.png`, which does not exist in
  `apps/web/public`. That is why the background is empty. Remove the broken
  reference or replace it with something real.
- `apps/web/src/components/header.tsx` and `footer.tsx` are shared with the
  changelog and projects pages. Restyle them only as far as the landing page
  needs, and check those pages still look right.
- Theme tokens live in `apps/web/src/styles.css` (light and dark, `--primary`
  is a blue, the handles use Tailwind `yellow-500`). Font is Inter Variable.
  `motion` is already a dependency. Icons come from `@hugeicons/core-free-icons`
  and `lucide-react`.
- Interface text is translated with `t('English text')` from
  `apps/web/src/i18n`; Chinese catalogues are in
  `apps/web/src/i18n/locales/zh-CN/`. The landing page is not wired to it yet.
- Brand: the ConzaCut marks in `apps/web/public/logos/conzacut`. See
  `ATTRIBUTION.md`.

## Required changes

1. Change the hero button label from "Try early beta" to **"Check it out"**.
   It still links to `/projects`.
2. Keep the trim-handle headline and make it work better: usable by keyboard
   (arrow keys move a focused handle), correct on touch, and no layout jump
   before the width is measured.
3. Give the page a real visual identity. Pick one clear direction and state it
   in two or three sentences before you build. Decide how the yellow trim
   accent and the blue primary relate, and use them consistently. The editor is
   a timeline tool, so timeline ideas (tracks, clips, playhead, waveforms,
   keyframe diamonds, safe-area guides) are the natural visual language. Draw
   them yourself in code or SVG.
4. Show the product. Below the hero, add a large, real view of the editor:
   screenshots or a short silent loop captured from the running app with media
   you generate yourself (test patterns, gradients, generated tones, your own
   text). Capture both themes if the page supports both. Serve them as AVIF or
   WebP with explicit dimensions, lazy-loaded below the fold.
5. Add sections that tell a visitor why to click, in this order unless you have
   a better one: what it does (a feature grid), private by design (media and
   projects stay in the browser), works where you are (web app, installable,
   native desktop app), programmable (Editor API, plugins, scripting, headless,
   MCP server, render CLI), export for every platform, open source (MIT, link
   to GitHub), and a closing call to action that repeats "Check it out".
6. Every feature you name must exist. Read `README.md`, `changelog/`,
   `apps/desktop/README.md` and the code, and list in your report where you
   confirmed each one. If the current line "Works on any platform" is not true
   (there is no mobile editor), rewrite it to what is true.
7. Wrap all new copy in `t()` and add the Chinese entries, following the
   existing catalogue files.
8. Update the page's meta description if the copy changes
   (`apps/web/src/site/brand.ts`).

## Quality bar

- Looks deliberate at 360, 768, 1280 and 1920 pixels wide, in light and dark.
- No generic template look: no purple gradient blobs, no stock illustrations,
  no emoji, no fake testimonials, logos or usage numbers.
- Motion has a purpose (scroll reveals, a playhead that moves, clips that
  snap). It respects `prefers-reduced-motion` and never blocks reading.
- Text contrast meets WCAG AA. One `h1`. Headings in order. All interactive
  elements reachable by keyboard with visible focus.
- The hero renders correctly from the server response, before JavaScript runs.
  No layout shift when images or fonts load.
- Keep it light: no new dependency unless you justify it in the report, no
  external fonts, scripts or trackers, no single asset over 1 MB, and nothing
  near the 25 MiB per-file limit of Workers static assets.

## Hard limits

- No CapCut or other proprietary editor's assets, screenshots, icons, fonts or
  copy. No stock media. No OpenCut marks on the site; ConzaCut marks only.
- The site does not mention OpenCut: not in the footer, the welcome dialog or
  the page copy. The repository keeps `LICENSE` and `ATTRIBUTION.md`, and the
  footer keeps its licence link, because MIT requires the original copyright
  notice to stay available.
- Do not touch the editor, the API worker, the desktop app or the crates.
- The working tree may hold other sessions' uncommitted changes. Do not stage,
  revert or reformat them. Commit only files you changed, on the current
  branch, and do not push.
- Do not deploy. Do not run `wrangler deploy` or `bun run deploy`.

## How to work and verify

1. Read the files above, run the app (`moon run web:dev`, port 5173) and look
   at the current page at the four widths in both themes.
2. State your design direction, then build it in
   `apps/web/src/components/landing/` as small section components.
3. Check in the browser at every width, both themes, English and Chinese, with
   reduced motion on, and by keyboard only. Fix what you see.
4. Run `moon run web:typecheck web:test web:build`.
5. Check `/changelog` and `/projects` still look right.

## Report

Finish with: the design direction you chose and why; before and after
screenshots at 1280 and 360 wide in both themes; the list of features claimed
on the page with the file that confirms each; any dependency added; what you
could not verify; and the commit hash. Then stop and wait for the owner to
approve a deploy.
