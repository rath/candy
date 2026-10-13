# Candy

<!-- impeccable:product-schema 1 -->

## Platform

Native macOS desktop application and Rust CLI (not a web or iOS app).

## Stack

Swift 6 and AppKit for the desktop interface. Rust `candy-core` shares pixel operations between CLI and GUI; `candy-macos` provides native Image I/O and Apple Vision integration; `candy-ffi` exposes the engine to Swift through a C ABI. The Rust `candy` executable provides terminal commands and launches the native app when needed.

## Users and purpose

People working in a terminal who want to inspect an image immediately and open a small editor only when needed. Success means opening a file, making a small edit, and saving a copy with minimal ceremony.

## Capabilities and constraints

Kitty terminal display; region extraction; region and OCR text mosaic; highlighter; resize; 90° rotation; transparency flattening. Open anything macOS Image I/O decodes, including PNG, JPEG, WebP, HEIC, AVIF, GIF, TIFF, BMP, PSD and camera RAW; export PNG, JPEG, HEIC, WebP, AVIF (where macOS encodes it), TIFF, GIF, BMP, JPEG 2000 and TGA. OCR stays on the Mac. Native document windows support macOS tabs, menus, save sheets, and unsaved-change confirmation. Full-resolution pixels remain authoritative even when the display preview is reduced.

First GUI save creates a new file; subsequent saves can update that output. Originals remain protected, including aliases. The CLI replaces its input only when asked with `--in-place`. macOS 14+; release binaries support Apple Silicon. MIT open source; public releases are Developer ID signed and notarized, while ordinary source builds are ad-hoc signed for local use. Homebrew installs the app and CLI together.

## Brand commitments

Name: Candy. A small, familiar Preview-like utility in Operate mode. The interface follows the macOS language, English, Korean or Japanese; support Korean and Japanese filenames and Korean/English/Japanese text detection. Follow system appearance, fonts, and controls; reserve custom coral accents for image regions.

## Product principles

- Return control to the shell after viewing or launching the editor.
- Keep the image in focus.
- Preserve originals by default.
- Share image operations between CLI and GUI.
- Use familiar macOS document and interaction conventions.
