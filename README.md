<p align="center"><img src="docs/assets/app-icon.png" width="128" height="128" alt="Candy app icon" /></p>

# Candy

A small image tool for your Mac. View an image in a Kitty-compatible terminal,
then open a lightweight editor when you need to change it.

```sh
candy photo.png
candy edit photo.png
```

Candy uses a native Swift/AppKit editor and a shared Rust image engine, with
macOS Image I/O and Apple Vision. Requires **macOS 14+**. Release binaries
support **Apple Silicon**.

## Install

Install the app and CLI together with Homebrew:

```sh
brew install --cask rath/tap/candy
```

Or download the ZIP from [GitHub Releases](https://github.com/rath/candy/releases/latest),
unzip it, and drag `Candy.app` to Applications. Release apps are signed with
Developer ID and notarized by Apple. Rust and Xcode are not needed to run them.
The Homebrew installation also puts `candy` on your PATH.

To update, quit Candy and run `brew update && brew upgrade --cask candy`.

## Build and run

Install Rust 1.95 or newer and Xcode 16 or newer (Swift 6), then build the
application and CLI together from this repository:

```sh
scripts/build-app.sh
./target/release/candy edit photo.png
```

Install just the CLI into Cargo's bin directory (make sure it is on your PATH):

```sh
cargo install --locked --path crates/candy
```

`cargo install` does not build the Swift editor. Build `dist/Candy.app` with the
script above and open it, or copy it to `/Applications` or `~/Applications`:

```sh
open dist/Candy.app
```

The bundle includes a CLI at `Contents/Helpers/candy`. The GUI executable is
`Contents/MacOS/Candy`; they are in separate directories for case-insensitive
macOS filesystems. The CLI uses `CANDY_APP` when set, otherwise a bundle beside
or containing the executable, a repository `dist/Candy.app`, or the app
registered with LaunchServices. An invalid `CANDY_APP` is reported explicitly.

```sh
CANDY_APP=/path/to/Candy.app candy edit photo.png
```

The script builds for the current architecture; `--debug` selects a debug build.
The bundle is ad-hoc signed for local use, not Developer ID signed or notarized.
Each build stages a clean bundle before replacing `dist/Candy.app`.
Open images from the File menu, drag and drop, Finder Open With, or the CLI.
The bundle declares supported document types without changing your default
image application.

## Terminal workflow

`candy photo.png` probes Kitty graphics support and displays a fitted preview
when available. Otherwise it launches the editor. `candy edit photo.png`
launches a detached editor and returns control to the shell immediately.
Running `candy` without arguments opens an empty editor.

```sh
candy --display gui photo.heic
candy --display kitty photo.avif
```

Inside tmux, previews travel through tmux passthrough, which needs
`set -g allow-passthrough on`, and are placed with Kitty Unicode placeholders
so tmux scrolls and redraws them like text. There, automatic detection asks
tmux instead of probing the terminal: it displays inline when passthrough is
enabled and tmux reports a Ghostty or kitty client, and launches the editor
otherwise. `--display kitty` skips capability detection. GNU screen passthrough
and the iTerm image protocol are not implemented. Redirected image-display
output is rejected rather than writing escape sequences into a file. Automatic
detection has been checked in Ghostty, both directly and inside tmux.

## Editing

- **Select:** drag a rectangle, move it or resize its corners, then extract it into a new tab.
- **Mosaic:** pixelate the selected rectangle with an adjustable block size.
- **Highlight:** draw translucent strokes with adjustable color, size and opacity.
- **Text mosaic:** detect text locally with Apple Vision, review or adjust boxes, then apply mosaic. Detection is script-independent; English, Korean and Japanese text are checked.
- **Resize:** set dimensions, optionally preserving the aspect ratio, or rotate by 90° in either direction.
- **Flatten alpha:** composite transparency onto a chosen solid color.

File, Edit, View and Tools actions live in the macOS menu bar. Each document
has a native window, toolbar, contextual controls, canvas, and undo/redo. Native
tabs follow your macOS preference; use Window → Merge All Windows or Move Tab
to New Window to group or separate documents.
The interface follows your macOS language: English, or Korean or Japanese when
macOS prefers one of them. Detailed error messages from the image engine stay in
English.
First Save asks for a new file; subsequent saves update that copy. Save As
creates another copy. The save panel has a Format selector for PNG, JPEG, HEIC,
WebP, AVIF (where macOS can encode it), TIFF, GIF, BMP, JPEG 2000 and
TGA. JPEG, HEIC, AVIF and JPEG 2000 offer a quality slider and numeric input
from 1–100. WebP offers lossless or lossy compression, with quality controls
in lossy mode. Each document remembers its settings by format after a successful
save, including for Save As; new documents start with the defaults.
Original input paths, including
symlink and hardlink aliases, are protected. Files are encoded before an atomic
replacement, and closing unsaved documents prompts to save or discard.

| Shortcut | Action |
| --- | --- |
| Cmd+O | Open image |
| Cmd+S / Shift+Cmd+S | Save / Save as |
| Cmd+Z / Shift+Cmd+Z | Undo / Redo |
| Cmd+W / Cmd+Q | Close document / Quit |
| Cmd+0 / Cmd+9 | Actual size / Zoom to fit |
| Cmd+1 … Cmd+6 | Select, Mosaic, Highlight, Text mosaic, Resize, Flatten alpha |
| Escape | Cancel a drag or clear selection |
| Delete / Backspace | Remove the selected OCR box |
| Space + drag | Pan |
| Scroll / pinch | Zoom |

Actual Size maps one image pixel to one display backing pixel (so a 1000 px
image occupies 500 logical points on a 2× Retina display). Fit never enlarges
images beyond that size; manual zoom ranges from 1% to 1600%. A new window sizes
itself to show its first image at Actual Size, or as large as the screen allows.

OCR boxes are locked while detection runs. Other tools remain available; editing
the image discards the old detection result. During quit confirmation all
documents are locked; cancellation restores interaction and opens queued files.

Text detection finds candidate bounds without transcribing the words. It uses
Vision's text-region detector with full-resolution pixels supplied directly,
avoiding PNG conversion and OCR language-model startup even on images without text. It can miss text or mistake graphics for text. Review the detected
regions before sharing an edited image;
mosaic is a visual effect, not a guarantee of irreversible redaction.

## CLI edits

Editing commands write to an output path (`-o`), whose extension selects the
format, or replace the input itself with `-i`/`--in-place`. Rectangle
coordinates are `x,y,width,height` in image pixels, measured from the top-left
after EXIF rotation.

```sh
candy resize photo.heic --width 1200 -o smaller.png
candy resize scan.tif --width 800 --height 600 -o exact.webp
candy rotate photo.heic --cw -o upright.heic
candy rotate photo.jpg --ccw -i
candy crop photo.png --rect 20,30,300,200 -o crop.heic
candy mosaic photo.png --rect 20,30,300,200 --block-size 16 -o mosaic.png
candy mosaic screenshot.png --text -o text-mosaic.png
candy flatten logo.png --background '#ffffff' -o flat.jpg
candy resize photo.png --width 1200 -o photo.jpg --quality 80
candy resize photo.png --width 1200 -o photo.webp --webp-mode lossy --quality 80
candy rotate photo.heic --cw -o rotated.heic --quality 90
```

One resize dimension preserves the aspect ratio; providing both sets the exact
size. `rotate` turns a quarter turn clockwise (`--cw`) or counterclockwise
(`--ccw`). Use `--force` to replace an existing output; it never permits overwriting
the input. `--in-place` is the one way to change the input: the result is
re-encoded in the file's own format and swapped in atomically, keeping the
file's permissions and following a symlink to the file it points at. Metadata
such as EXIF is not carried over, and lossy formats are recompressed, so prefer
`-o` when the original matters. Run `candy --help` or `candy <command> --help`
for details.

All CLI editing commands accept `--quality <1–100>` for JPEG, lossy WebP,
HEIC, AVIF and JPEG 2000. WebP defaults to lossless; select
`--webp-mode lossy` to use quality (80 when omitted), or `--webp-mode lossless`
to explicitly request lossless output. Quality with lossless WebP, quality on
other formats, and WebP mode on a different format are errors. A quality of 100
does not select WebP lossless compression.
HEIC and AVIF quality 100 map to Image I/O quality 0.999 to avoid native
encoder failures at exactly 1.0 on some macOS versions and hardware.

`candy inspect photo.heic` reads the stored format, dimensions, EXIF
orientation, color model, bit depth, alpha, color profile and DPI from the
file's headers, then reports statistics of the pixels Candy would edit (8-bit
sRGB after orientation): opaque, translucent and transparent counts, the
alpha-weighted average color and the number of distinct colors. `--json`
prints the same report as one JSON object; `pixels` is `null` and
`pixels_error` explains why when the image cannot be decoded.

## Formats and current limits

| Operation | Formats |
| --- | --- |
| Open | Anything Image I/O on your Mac decodes: PNG, JPEG, WebP, HEIC/HEIF, AVIF, GIF, TIFF, BMP, JPEG 2000, PSD, TGA, ICO, ICNS, OpenEXR, Radiance HDR, PBM, camera RAW and DNG, and JPEG XL on macOS 26 or later; plus SVG and SVGZ, rasterized by resvg |
| Save | PNG, JPEG (default quality 92), WebP (default lossless; lossy default quality 80), HEIC, AVIF and JPEG 2000 (default quality 85), TIFF (LZW), GIF (256 colors), BMP, TGA |

Images are edited as 8-bit sRGB RGBA. EXIF orientation is applied on load;
source metadata, HDR data and wide-gamut profiles are not retained on export.
JPEG exports flatten transparency onto white; GIF keeps pixels at or above 50%
alpha and drops the rest. AVIF saving needs an Image I/O with an AVIF encoder,
which arrived after macOS 14 (verified on macOS 26); older systems explain this
when asked. RAW files are developed with Image I/O's defaults. Only the first
image/frame is loaded. Images are limited to approximately 64 megapixels. The GUI preview is
limited to 4096 pixels per side; edits and saves use full image resolution.
Lossy WebP additionally limits each side to 16,383 pixels. Its encoder is
included in the app and CLI; no external encoder installation is needed.

SVG files are rendered with the bundled resvg at their own size, enlarged so
the longer side is at least 1024 pixels and reduced to fit the pixel limit, with
transparency preserved. Text uses the fonts installed on your Mac. Nested SVG
`<image>` references resolve against the file's directory; embedded or linked
raster images (PNG, JPEG, GIF, WebP) inside an SVG are skipped to keep the
binaries small. `candy inspect` reports the rendered size. Edited SVGs save in a
raster format; Candy does not write SVG.

This version has no layers, animation editing, cloud services or LLM integration.
Text detection runs locally and sends no image data to a service. Detection quality and
codec availability depend on the installed macOS version.

## Development

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
scripts/build-app.sh --debug
scripts/swift.sh test
scripts/build-app.sh
```

- `candy-core`: pixel operations, edit history and atomic export.
- `candy-macos`: shared Image I/O decoding and Apple Vision text detection.
- `candy-ffi`: synchronous C ABI, owned document/image handles, bounded previews.
- `candy`: Rust CLI, Kitty graphics protocol and native-app launcher.
- `macos`: SwiftPM AppKit application, document coordination and canvas interaction.

The Swift target links the Rust engine statically. Build `candy-ffi` before running
Swift tests alone (`cargo build --locked -p candy-ffi`). `scripts/swift.sh` keeps
Swift caches in `macos/.build` and disables the nested package-manifest sandbox
so it can run inside an existing development sandbox. Native OCR and UI checks
need normal macOS access to Vision/pixel buffers and the window server.

Interface text uses the English text as its key (`localized(_:)` in CandyKit).
Korean and Japanese translations live in `macos/Resources/ko.lproj/` and
`macos/Resources/ja.lproj/Localizable.strings`; a Swift test fails when an entry
is missing, unused or has different placeholders, or when `scripts/build-app.sh`
does not ship a table. To see a translated interface without changing system
settings, pass the language to that process only:

```sh
open -n -a dist/Candy.app --args -AppleLanguages '(ko)'
open -n -a dist/Candy.app --args -AppleLanguages '(ja)'
```

Debug builds support a process-only appearance override for visual checks:

```sh
CANDY_TEST_APPEARANCE=dark dist/Candy.app/Contents/MacOS/Candy
```

Quit any existing test instance first. This does not change system preferences
and is not included in release builds.

Tests cover edit boundaries, alpha compositing, save protection, undo state,
terminal encoding, FFI ownership, native document/async-state regressions, a save-and-reopen
round trip through every export format, PSD/ICO/ICNS/OpenEXR/PBM decoding, SVG
rasterization, EXIF
orientation and Korean/English text-region coverage. Synthetic fixtures can be regenerated with
`swift scripts/make-fixtures.swift crates/candy-macos/tests/fixtures`.
GitHub Actions runs the checks and produces a zipped local app bundle on macOS.

The application icon is committed as `macos/Resources/AppIcon.icns`. Its sources
are `scripts/icon/app-icon.svg` and the simplified 16-point
`scripts/icon/app-icon-small.svg`. After editing either, regenerate it with
`scripts/build-icon.sh` (requires `rsvg-convert` from Homebrew `librsvg`).

To profile text detection on synthetic blank, graphical, Korean/English and large
images (requires normal macOS Vision access):

```sh
CANDY_OCR_TIMING=1 cargo run --locked --release -p candy-macos --example ocr-timing
```

The opt-in log separates image preparation from Vision execution and reports no
image contents. Timings depend on the machine and macOS version.

Made by [Jang-Ho Hwang](https://xrath.com). MIT licensed. See [PRODUCT.md](PRODUCT.md) for scope and [DESIGN.md](DESIGN.md)
for the editor's visual conventions.

Maintainers: see [releasing](docs/releasing.md) for signing, notarization,
GitHub Releases, and Homebrew distribution.

The GitHub Pages landing page in English, Korean and Japanese is in `site/`. See [website maintenance](docs/website.md) for preview, publishing, and DNS setup.
