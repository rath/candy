# Website

The landing page lives in `site/`: English at `/`, Korean at `/ko/`, Japanese
at `/ja/`. All three pages are plain HTML sharing one stylesheet, each with a
recorded app demo in its own language. No JavaScript, package installation,
analytics, or third-party font request is required; a few inline lines only add
a pause button to the demo. Update all three pages when changing product copy.

Preview locally:

```sh
python3 -m http.server 4180 --directory site
```

Visit http://localhost:4180, http://localhost:4180/ko/ and
http://localhost:4180/ja/.

## Publish

Pushing `main` publishes the site. The `Website` workflow
(`.github/workflows/pages.yml`) runs when a push changes `site/` or the workflow
itself, uploads the committed `site/` directory as is, and deploys it to GitHub
Pages. There is no build step, so commit finished HTML, CSS and assets. Pushes
that change only source code or documentation leave the live site untouched. To
redeploy without a change, run `gh workflow run pages.yml`; follow runs with
`gh run list --workflow pages.yml`.

The repository's Pages source is **GitHub Actions**, and the `github-pages`
environment allows deployments from `main`. The `gh-pages` branch is no longer
published.

The custom domain `candy.told.me` is set in the repository's Pages settings.
`site/CNAME` also records it, but Actions deployments ignore that file. The
domain owner must configure this DNS record:

| Type | Name | Target |
| --- | --- | --- |
| CNAME | candy | rath.github.io |

The target has no protocol, repository suffix, or path. After DNS resolves and
GitHub provisions the certificate, enable **Enforce HTTPS** in Pages settings.
Check status with `gh api repos/rath/candy/pages`. Do not assume that publishing
files also provisions DNS.

Canonical URLs, language alternates, sitemap and social metadata use the custom
domain. The demo edits made-up scenes from `scripts/demo/scenes/`; provenance
is recorded in `site/assets/credits.txt`.

## Re-record the demo

The demo videos, their posters and the social cards show the real app, so a release that changes
what the editor looks like needs a new recording. `scripts/demo/` holds everything:

```sh
scripts/build-app.sh                                   # the release build is what gets recorded
CANDY_DEMO_DRY_RUN=1 scripts/demo/record-demo.sh       # every scene, without touching the mouse
scripts/demo/record-demo.sh                            # takes over the mouse for about four minutes
python3 scripts/demo/build-video.py                    # writes site/assets/candy-demo-*.mp4 and candy-editor-*
scripts/build-og.sh                                    # the social cards use the new stills
```

`render-scenes.sh` renders the scenes at 2x in a headless browser and records, through
`targets.js`, where each mosaic, highlight and eyedropper pick goes. `record.swift` opens each
scene in a new Candy instance with the page's language, sets the highlighter to 40 px at 45 % and
the mosaic to 14 px blocks through accessibility, then moves the real pointer through the edits
while ScreenCaptureKit records the display with every other window excluded. Capture the display,
not Candy's windows: a filter that names the window makes macOS draw a sharing badge over the
window buttons. Raw movies and stills stay in `.impeccable/review/demo/` (ignored), so
`build-video.py` can be tuned without recording again.

The recording moves the pointer and brings Candy to the front, which the native UI rules
otherwise forbid: run it only after the user has agreed to leave the Mac alone, and check each
language in the output before committing. Update `site/assets/credits.txt` with the version and
date, and the `width`/`height` of the video and images if the window size changes.

## Search and sharing

Every page has localized search titles and descriptions, self-referencing
canonical URLs, reciprocal `en` / `ko` / `ja` / `x-default` language
alternates, and indexable robots metadata allowing large image previews.
`robots.txt` advertises the trilingual sitemap. Update its `lastmod` dates after
meaningful page changes.

Open Graph and Twitter large-image cards use absolute URLs to separate
`assets/og-en.png`, `assets/og-ko.png` and `assets/og-ja.png` images
(1200 × 630), with localized alt text, explicit dimensions, and alternate
locales. JSON-LD describes the website,
each localized page, and the free macOS application. It intentionally contains
no invented reviews or ratings. Keep platform and pricing facts in sync with
the visible page when changing them.

### Rebuild the social cards

The editable layout is `scripts/og/index.html`, outside the published `site/`
tree. It combines real app imagery with HTML/CSS typography; text is not baked
by an image generator. On macOS with Python 3 and the installed `agent-browser`
CLI, run this one command (no image API credentials or font downloads needed):

```sh
scripts/build-og.sh
```

The script starts an isolated loopback server on a free port, uses a visible
browser with the `candy` session, waits for fonts and images, and captures only
the card. It validates all three 1200 × 630 PNGs before replacing any committed
asset, then cleans up its temporary files and server. Rendering is not
byte-for-byte deterministic, so restore cards whose language copy did not change
(`git checkout -- site/assets/og-en.png`) to keep the diff to the intended
ones. It leaves other preview
servers and the shared browser session running.

To reuse a running preview, set `CANDY_PREVIEW_URL` to a server that serves the
**repository root**, not `site/`. If size validation fails, use a browser session
with device scale 1. Inspect every language version after changing copy or the
app screenshot. Commit the regenerated cards; pushing `main` publishes them with
the rest of `site/`.
