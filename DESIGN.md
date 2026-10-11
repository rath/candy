---
name: Candy editor
description: A compact AppKit workspace for quick image edits.
colors:
  accent-light: "rgb(71.8% 27.5% 19.6%)"
  accent-dark: "rgb(100% 60.8% 51.4%)"
  checker-light-a: "rgb(96% 96% 96%)"
  checker-light-b: "rgb(91% 91% 91%)"
  checker-dark-a: "rgb(21% 21% 21%)"
  checker-dark-b: "rgb(16% 16% 16%)"
  handle-fill: "#FFFFFF"
typography:
  label:
    fontFamily: "macOS system font"
    fontSize: "12pt"
  value:
    fontFamily: "macOS monospaced-digit system font"
    fontSize: "12pt"
rounded:
  region: "0pt"
spacing:
  options-gap: "10pt"
  status-gap: "8pt"
  horizontal-inset: "12pt"
  options-vertical-inset: "10pt"
  status-vertical-inset: "6pt"
  empty-gap: "12pt"
components:
  options:
    padding: "10pt 12pt"
    height: "54pt"
  status:
    padding: "6pt 12pt"
    height: "32pt"
  slider:
    width: "115pt"
  slider-value:
    typography: "{typography.value}"
    width: "36pt"
  dimension-field:
    width: "76pt"
  selection-handle:
    backgroundColor: "{colors.handle-fill}"
    rounded: "{rounded.region}"
    size: "7pt"
---

# Design System: Candy editor

## Overview

**Creative North Star: "A familiar Preview-like image workspace"**

Candy is a native macOS image utility in Operate mode: a familiar Preview-like workspace with the image in focus. A native toolbar and contextual controls sit above the canvas; a compact status strip sits below it. Document windows support native macOS tabs and follow the system light or dark appearance.

**Key Characteristics:**

- Native AppKit controls, menus, document windows, and sheets.
- System fonts and semantic colors for ordinary interface elements.
- Coral outlines for custom image regions.
- Visible tool values, document state, progress, and errors.

This records the implemented Swift 6/AppKit interface. Lengths are macOS logical points, not web CSS instructions. The source of truth is `macos/Sources/CandyKit/EditorWindow.swift`, `Canvas.swift`, `SaveFormatView.swift`, and `Application.swift`. Native screenshot evidence is listed in `.impeccable/design.json`; it covers light and dark appearance, compact native tabs, OCR, and save/close sheets. The dark capture shows a settled highlight edit at compact window size. No web detector or web/iOS platform mapping is used.

## Colors

### Primary

Warm coral changes with system appearance. `accent-light` and `accent-dark` are the explicit sRGB components used for custom region outlines. OCR boxes use the same accent with 0.12 alpha. Native tool selection, tabs, and buttons retain AppKit's appearance and system accent behavior.

### Neutral

The checker pairs indicate transparency inside the visible image area; their tokens record the white-component values passed to `NSColor(white:alpha:)`. `handle-fill` fills square region handles. The surrounding canvas uses `NSColor.underPageBackgroundColor`. Status text and the supported-format line use `secondaryLabelColor`; errors use `systemRed`. Standard surfaces, control text, disabled states, and separators remain system-defined. Do not convert these semantic colors into a fixed light/dark palette.

Highlight color defaults to `systemYellow` at 0.4 opacity and the flattening background defaults to white. These are editable image-operation defaults, not an interface accent scheme.

## Typography

Use AppKit system fonts and its native fallback for Korean and other scripts. Do not bundle font files. Most controls and ordinary labels inherit AppKit's typography. The explicit overrides are regular contextual labels and regular monospaced-digit slider values, as recorded in the frontmatter; the empty-state instruction uses the default label font. Status uses `NSFont.smallSystemFontSize`; its numeric size is intentionally not redefined here. Line heights, letter spacing, and native control weights are not overridden.

## Layout

The initial content rectangle is (1120 × 780 pt), before the native titlebar and toolbar. A window opened for a file stays hidden until the image has loaded, or failed, and then appears already sized around its center so Fit shows the image at Actual Size; an empty window that receives a file resizes in place. An image larger than the screen's visible frame shrinks on both axes together, and the window stays inside that frame. Tabbed, full-screen and user-resized windows keep their size; later edits never resize the window. `NSWindow.contentMinSize` is (560 × 268 pt), a (560 × 320 pt) window with the unified toolbar; `minSize` is avoided because it is converted before the toolbar lays out. The width fits the widest contextual strip and the tool selector. A unified native toolbar centers the tool selector between flexible spaces. AppKit owns titlebar and tab-bar layout.

Below the toolbar, a fixed-height contextual strip uses a horizontal `NSStackView` inside an `NSScrollView`. It scrolls horizontally when needed and autohides its scroller; controls do not wrap. The canvas fills the remaining area above a native separator and fixed-height status strip. The frontmatter records actual strip heights, insets, gaps, and control widths. There are no custom breakpoints. The empty state is a centered vertical stack containing one instruction line and the Open image action, nothing else.

Fit centers the image with (40 pt) total allowance per axis and never scales above one image pixel per backing pixel. Actual Size is exactly one image pixel per backing pixel, including on Retina displays. Manual zoom ranges from (1%–1600%). Scrolling and trackpad magnification zoom around the pointer; Space-drag pans. A preview is capped at 4096 pixels per side, while coordinates, editing, and exports use the full-resolution oriented image.

## Elevation & Depth

Native titlebars, controls, color panels, popups, and sheets provide their own depth. The custom canvas has no shadow treatment. A native separator distinguishes the status strip. Progress uses a small spinning `NSProgressIndicator`; no decorative application animation is defined.

## Shapes

Image rectangles, checker cells, region outlines, and handles have square corners. Checker cells are (12 pt) squares in screen space. Buttons use AppKit's rounded bezel style; their actual radius and focus treatment are system-defined. Text fields, the segmented selector, color wells, and sheets retain their native shapes.

## Components

### Menus and tool selector

The macOS menu bar provides file, text-editing, history, view, tool, and window commands with native key equivalents. A centered `NSSegmentedControl` selects Select, Mosaic, Highlight, Text mosaic, Resize, or Flatten alpha. It uses SF Symbol icons with tooltips and accessibility descriptions naming each tool, and single selection. Menu check marks mirror the active tool. Commands and controls disable when their document state makes them unavailable.

### Document windows and tabs

Each document has its own `NSWindow` and editing session. Windows share a tabbing identifier and use automatic native tabbing. Show Tab Bar, Merge All Windows, and Move Tab to New Window expose AppKit's tab behavior. Native window titles, represented URLs, and `isDocumentEdited` communicate identity and unsaved changes; do not draw a replacement tab strip or dirty marker.

### Contextual controls

Select exposes region instructions and Extract to new image. Mosaic exposes block size and Apply mosaic. Highlight exposes a native color well, width, and opacity. Text mosaic exposes Detect text, block size, Apply to all, and Remove box; region count appears in status. Resize exposes width and height text fields, Keep ratio, Apply resize, and icon-only Rotate left and Rotate right buttons (`rotate.left`/`rotate.right`) whose tooltips name the 90° direction; a rotation swaps the field values. Flatten alpha exposes a background color well and its apply action.

Sliders have adjacent right-aligned numeric values; opacity displays as a percentage. Image measurements use pixels. Block size ranges from 2–96, brush width from 2–100, and resize dimensions accept 1–65535 subject to engine allocation limits. During OCR detection, text-region controls and canvas region editing lock, while other tools remain available. An edit or save disables document editing until it completes.

### Canvas and region overlays

Active regions have a (2 pt) coral outline and square white corner handles; inactive OCR regions have a (1 pt) outline. Handle hit detection uses a (9 pt) radius. OCR boxes also receive the translucent accent fill described above. Live brush strokes have round caps and joins and are clipped to the image/canvas intersection. The canvas uses a crosshair; holding Space changes it to an open hand. Escape cancels an active drag and restores the prior regions, or clears the selection/active box when idle. Delete removes the active OCR box when region editing is enabled.

### Status and native sheets

Status combines image dimensions, zoom, selected-region coordinates and dimensions, OCR region count where applicable, and the latest message. It truncates in the middle and exposes the full text in a tooltip. Progress appears alongside it. Errors use the system error color and a small Dismiss button.

Open uses `NSOpenPanel`. Saving uses a document-scoped `NSSavePanel` with a visible Format popup offering PNG, JPEG, HEIC, WebP (lossless), AVIF where macOS can encode it, TIFF, GIF, BMP, JPEG 2000 and TGA; changing the format updates the filename extension. The first save creates a copy. Closing a modified document presents an `NSAlert` sheet with Save copy… (or Save for an existing output), Cancel, and Discard. Quit confirms stable document identities, blocks editing across all windows, and queues new Open events. Cancel restores window controls and processes queued opens.

### Language

Interface text follows the macOS language: English, or Korean when macOS prefers it. Korean uses macOS's own terms for standard menu and document commands, as AppKit and Preview translate them, and the website's tool names (형광펜, 모자이크, 배경 채우기). Component names in this document are the English titles. Error details passed through from the Rust engine remain English.

### Application icon

Finder, the Dock, and the About panel show `macos/Resources/AppIcon.icns`: a cream lollipop with a coral Archimedean spiral (2.25 turns), tilted −18° on a deep rust superellipse. The superellipse fills the standard 824 px area of the 1024 px canvas, so current macOS presents it without an added backing plate. The 16 pt slot uses simplified artwork with 1.25 turns and a thicker stick; 32 pt and larger use the full artwork. Colors are `#9C4432`–`#6B2A1E` (background gradient), `#FFF4EC` (cream), and `#F08A72` (coral). They identify the application outside its windows and are not interface tokens.

## Do's and Don'ts

### Do:

- Do keep the canvas as the main workspace.
- Do use native AppKit controls and their system appearance.
- Do reserve the custom coral palette for image-region overlays.
- Do keep numeric values and units visible beside editing controls.
- Do preserve keyboard commands, native tabs, and document-scoped sheets.

### Don't:

- Don't add decorative cards or dashboard composition to the editor.
- Don't treat colors or typography inside an opened image as Candy interface tokens.
- Don't replace system colors, font metrics, control shapes, or focus states with guessed values.
- Don't use preview dimensions for edits or exports.
