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
