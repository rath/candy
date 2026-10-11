# Website

The landing page lives in `site/`: English at `/`, Korean at `/ko/`. Both pages
are plain HTML sharing one stylesheet and a real app screenshot. No JavaScript,
package installation, analytics, or third-party font request is required.
Update both pages when changing product copy.

Preview locally:

```sh
python3 -m http.server 4180 --directory site
```

Visit http://localhost:4180 and http://localhost:4180/ko/.

## Publish

GitHub Pages serves the root of the `gh-pages` branch. Publish only the committed
contents of `site/` from the repository root:

```sh
git subtree push --prefix site origin gh-pages
```

This preserves history and does not force-push. Commit website changes before
publishing. Source code and documentation remain on `main`; push that branch
when publishing changes that visitors need to build the current app.

The custom domain is `candy.told.me`, recorded in `site/CNAME` and GitHub Pages
settings. The domain owner must configure this DNS record:

| Type | Name | Target |
| --- | --- | --- |
| CNAME | candy | rath.github.io |

The target has no protocol, repository suffix, or path. After DNS resolves and
GitHub provisions the certificate, enable **Enforce HTTPS** in Pages settings.
Check status with `gh api repos/rath/candy/pages`. Do not assume that publishing
files also provisions DNS.

Canonical URLs, language alternates, sitemap and social metadata use the custom
domain. The screenshot uses synthetic fixture content; provenance is embedded
in the PNG and recorded in `site/assets/credits.txt`.

## Search and sharing

Both pages have localized search titles and descriptions, self-referencing
canonical URLs, reciprocal `en` / `ko` / `x-default` language alternates, and
indexable robots metadata allowing large image previews. `robots.txt` advertises
the bilingual sitemap. Update its `lastmod` dates after meaningful page changes.

Open Graph and Twitter large-image cards use absolute URLs to separate
`assets/og-en.png` and `assets/og-ko.png` images (1200 × 630), with localized alt
text, explicit dimensions, and alternate locales. JSON-LD describes the website,
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
the card. It validates both 1200 × 630 PNGs before replacing either committed
asset, then cleans up its temporary files and server. It leaves other preview
servers and the shared browser session running.

To reuse a running preview, set `CANDY_PREVIEW_URL` to a server that serves the
**repository root**, not `site/`. If size validation fails, use a browser session
with device scale 1. Inspect both language versions after changing copy or the
app screenshot. Publishing follows the same `gh-pages` subtree process above;
source-only changes do not update the live site.
