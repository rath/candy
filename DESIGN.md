---
name: Candy editor
description: A compact native macOS workspace for quick image edits.
colors:
  accent-light: "#B74632"
  accent-dark: "#FF9B83"
  canvas-light: "#E2E2E2"
  canvas-dark: "#181818"
  checker-light-a: "#F6F6F6"
  checker-light-b: "#EAEAEA"
  checker-dark-a: "#363636"
  checker-dark-b: "#2A2A2A"
  handle-fill: "#FFFFFF"
typography:
  headline:
    fontFamily: "SF Pro (system), egui proportional fallback"
    fontSize: "22pt"
  heading:
    fontFamily: "SF Pro (system), egui proportional fallback"
    fontSize: "15pt"
  body:
    fontFamily: "SF Pro (system), egui proportional fallback"
    fontSize: "13pt"
  label:
    fontFamily: "SF Pro (system), egui proportional fallback"
    fontSize: "13pt"
  small:
    fontFamily: "SF Pro (system), egui proportional fallback"
    fontSize: "11pt"
rounded:
  control: "6pt"
  canvas: "0pt"
  selection-handle: "1pt"
spacing:
  item-horizontal: "8pt"
  item-vertical: "6pt"
  button-horizontal: "10pt"
  button-vertical: "4pt"
  panel-inset: "12pt"
  section: "12pt"
components:
  button:
    typography: "{typography.label}"
    padding: "4pt 10pt"
    rounded: "{rounded.control}"
  canvas-light:
    backgroundColor: "{colors.canvas-light}"
    rounded: "{rounded.canvas}"
  canvas-dark:
    backgroundColor: "{colors.canvas-dark}"
    rounded: "{rounded.canvas}"
  selection-handle:
    backgroundColor: "{colors.handle-fill}"
    rounded: "{rounded.selection-handle}"
    size: "7pt"
---

# Design System: Candy editor

## Overview

**Creative North Star: "A familiar Preview-like image workspace"**

Candy is a native macOS image utility in Operate mode: a familiar Preview-like workspace with the image in focus. File, history, view and tool commands live in the macOS menu bar; the window holds a tool strip with contextual options, document tabs when more than one image is open, the canvas, and a status strip. The application follows the system light or dark appearance.

**Key Characteristics:**
- Native menu bar for commands; text labels for in-window tool selection.
- The macOS system font at macOS control sizes.
- Coral selection accents on neutral editor surfaces.
- One shared horizontal inset so panels line up down the window.
- Visible document, selection, progress, and error state.

This document records the implemented native egui interface. Token lengths use egui logical points; they are not CSS sizing instructions. Source of truth: `crates/candy/src/gui.rs` (`configure`, `menus`, `toolbar`, `tabs`, `options`, `status`, `close_dialog`, and `ui`), `crates/candy/src/canvas.rs` (`Canvas::show`) and `crates/candy-macos/src/menu.rs`. The light appearance was visually checked against the `.impeccable/review/layout-*.png` captures. Dark colors are verified from code, not those captures.

## Colors

### Primary

Warm coral changes with system appearance. `accent-light` and `accent-dark` drive the selection stroke color for egui selectable controls and the custom image-region outlines. Selected-control fill uses the current accent with `Color32::gamma_multiply(0.22)`; OCR-region fill uses `gamma_multiply(0.12)`. These are egui color operations, not CSS opacity declarations.

### Neutral

The `canvas` pair fills the surrounding workspace. Each `checker` pair indicates image transparency within the image bounds. `handle-fill` fills the selection's corner handles. Panel, control, ordinary text, disabled text, divider, and error colors remain inherited from egui's light/dark visuals; Candy does not define separate palette values for them.

## Typography

The egui Proportional family is loaded at runtime from the Mac: the system UI font (SF Pro, `/System/Library/Fonts/SFNS.ttf`) first, then egui's bundled fonts for symbols and emoji, then Apple SD Gothic Neo as the Korean fallback. No font file is redistributed; if a system font is missing, egui's defaults remain.

- **Headline:** Empty-state message, using egui's strong treatment.
- **Heading:** The unsaved-changes modal title.
- **Body:** Ordinary instructions, dimensions, and field labels.
- **Label:** Buttons, tabs and editing controls.
- **Small:** The status strip and the supported-format line.

Weights, line heights, and letter spacing are not overridden. Text in an opened image is document content, not part of this hierarchy.

## Layout

The initial viewport is (1120 × 780 pt), with a minimum inner size of (760 × 520 pt). From top to bottom: document tabs (only with two or more images); the toolbar panel holding the tool strip and, when a document is open, the active tool's options (6 pt) below it; the canvas; a bottom status strip. Every panel uses the (12 pt) horizontal inset; the toolbar uses (8 pt) vertical padding and the tab and status strips (4 pt).

Control rows use `horizontal_wrapped`; tabs use a horizontal scroll area. There are no custom breakpoints. Item spacing is (8 pt) horizontally and (6 pt) vertically, tightened to (4 pt) between tool labels and (2 pt) between a tab and its close control. The interaction-height baseline is (24 pt).

The canvas takes remaining space. Fit centers the image with (40 pt) total allowance in each axis and does not enlarge it beyond its native dimensions. Manual zoom ranges from (1%–1600%); panning preserves image-coordinate editing. The empty state uses centered text, an Open image action, and a supported-format line, separated by (8/16/12 pt) gaps.

## Elevation & Depth

The workspace uses neutral fills, panel dividers, and state accents. The canvas uses `Frame::NONE`; no custom editor shadows are defined. The unsaved-changes modal and standard egui popups retain their library elevation and interaction styling. There are no application-defined decorative animations; progress uses egui spinners.

## Shapes

The canvas, checkerboard, OCR boxes, and selected-image rectangles have square corners. Selection corner handles use the documented handle shape. Other button, panel, input, and modal radii are inherited from egui; the provisional seed's radius values were not implemented as Candy overrides.

## Components

### Menu bar

Open, Close Tab, Save, Save As, Undo, Redo, Actual Size, Zoom to Fit and the six tools are native menu items with Cmd key equivalents (File, Edit, View, Tools), beside the standard application and Window menus. Items disable when unavailable, while the document is busy, and while the unsaved-changes prompt is open; the active tool carries a check mark.

### Tool strip

Selectable labels for Select, Mosaic, Highlight, Text mosaic, Resize, and Flatten alpha. Selection colors come from the coral theme overrides; hover, focus, and disabled rendering otherwise remain standard egui behavior.

### Document tabs

Shown only when more than one image is open. Selectable filename labels carry the coral active treatment, a trailing dot marks unsaved edits, and a frameless × closes the tab. The native window title always reflects the active filename and dirty marker.

### Contextual controls

The options row uses egui buttons, sliders, color controls, drag-value fields, and a ratio checkbox as needed by the selected tool. Image measurements are labeled in pixels. OCR exposes detection, region count, block size, Apply to all, and Remove box; regions remain visible for adjustment before applying.

### Canvas and region overlays

Transparency cells are (12 pt) squares in screen space. Ordinary selection outlines use a (1.5 pt) inside stroke. OCR regions use a (1 pt) outline, with (2 pt) for the active region, plus the accent fill described above. The selected region receives white corner handles with a (1 pt) accent border. Handle hit detection uses a (9 pt) radius around each corner. The pointer becomes a crosshair for selection/highlight or a grab cursor while Space is held.

### Status and confirmations

The bottom strip uses the small text style: image dimensions, zoom and the selected region joined by dots on the left; progress with a (12 pt) spinner, or otherwise the latest notice in weak text, on the right. Errors replace the left group in egui's error foreground with a Dismiss action. An unsaved-changes modal has a (400 pt) maximum content width and Save copy, Discard, and Cancel actions. File selection uses native dialogs.

## Do's and Don'ts

### Do:
- Do keep the canvas as the main workspace.
- Do use coral for selected tools, selected documents, and image-region outlines.
- Do preserve system-following light and dark appearances.
- Do retain text labels, contextual options, and visible status feedback.
- Do put new commands in the menu bar with a Cmd shortcut rather than adding window buttons.

### Don't:
- Don't add decorative cards or dashboard composition to the editor.
- Don't treat the colors or typography inside an opened image as Candy interface tokens.
- Don't replace inherited egui styles with guessed native or web values.
- Don't bundle fonts; load system fonts at runtime and keep egui's defaults as the fallback.
