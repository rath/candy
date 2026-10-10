# Candy

<!-- impeccable:product-schema 1 -->

## Platform

Native macOS desktop application and CLI (not a web or iOS app).

## Stack

Rust, egui/eframe with wgpu, Image I/O and Apple Vision through Rust bindings.

## Users and purpose

People working in a terminal who want to inspect an image immediately and open a small editor only when needed. Success means opening a file, making a small edit, and saving a copy with minimal ceremony.

## Capabilities and constraints

Kitty terminal display; region extraction; region and OCR text mosaic; highlighter; resize; transparency flattening. Open PNG, JPEG, WebP, HEIC and AVIF; export PNG, JPEG and WebP. OCR stays on the Mac. First save creates a new file. macOS 14+; initially validated on Apple Silicon. MIT open source; no publishing requested.

## Brand commitments

Name: Candy. User requested a small, familiar Preview-like utility, not a large creative suite. Use English UI for this draft; support Korean filenames and Korean/English OCR.

## Product principles

- Return control to the shell after viewing or launching the editor.
- Keep the image in focus.
- Preserve originals by default.
- Share image operations between CLI and GUI.
