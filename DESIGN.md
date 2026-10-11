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
    fontFamily: "egui proportional"
    fontSize: "24pt"
  title:
    fontFamily: "egui proportional"
    fontSize: "17pt"
  body:
    fontFamily: "egui proportional"
    fontSize: "14pt"
  label:
    fontFamily: "egui proportional"
    fontSize: "14pt"
  small:
    fontFamily: "egui proportional"
    fontSize: "12pt"
rounded:
  canvas: "0pt"
  selection-handle: "1pt"
spacing:
  item: "8pt"
  button-horizontal: "10pt"
  button-vertical: "6pt"
  section: "12pt"
components:
  button:
    typography: "{typography.label}"
    padding: "6pt 10pt"
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

Candy is a native macOS image utility in Operate mode: a familiar Preview-like workspace with the image in focus. Compact text controls, contextual options, document tabs, and a status strip surround the canvas. The application follows the system light or dark appearance.

**Key Characteristics:**
- Text labels for editing tools and file actions.
- Coral selection accents on neutral editor surfaces.
- A flexible canvas with compact, wrapping control rows.
- Visible document, selection, progress, and error state.

This document records the implemented native egui interface. Token lengths use egui logical points; they are not CSS sizing instructions. Source of truth: `crates/candy/src/gui.rs` (`configure`, `toolbar`, `tabs`, `options`, `status`, `close_dialog`, and `ui`) and `crates/candy/src/canvas.rs` (`Canvas::show`). The light appearance and compact-window composition were visually checked against `.impeccable/review/editor-final.png` and `.impeccable/review/editor-small-final.png`. Dark colors are verified from code, not those captures.

## Colors

### Primary

Warm coral changes with system appearance. `accent-light` and `accent-dark` drive the selection stroke color for egui selectable controls and the custom image-region outlines. Selected-control fill uses the current accent with `Color32::gamma_multiply(0.22)`; OCR-region fill uses `gamma_multiply(0.12)`. These are egui color operations, not CSS opacity declarations.

### Neutral

The `canvas` pair fills the surrounding workspace. Each `checker` pair indicates image transparency within the image bounds. `handle-fill` fills the selection's corner handles. Panel, control, ordinary text, disabled text, divider, and error colors remain inherited from egui's light/dark visuals; Candy does not define separate palette values for them.

## Typography

The logical font family is egui Proportional with its default font definitions. Candy appends the installed Apple SD Gothic Neo font as a Korean fallback when that file is available. It does not replace the default font with a macOS system UI font.

- **Headline:** Empty-state message, using egui's strong treatment.
- **Title:** Candy toolbar label, also using strong treatment.
- **Body:** Ordinary instructions, dimensions, and status values.
- **Label:** Buttons and editing controls.
- **Small:** The status notice.

Weights, line heights, and letter spacing are not overridden. Modal headings retain egui's Heading style. Text in an opened image is document content, not part of this hierarchy.

## Layout

The initial viewport is (1120 × 780 pt), with a minimum inner size of (760 × 520 pt). From top to bottom: file/history/view controls and a tool row; horizontally scrolling document tabs; contextual tool options; the canvas; a bottom status strip. Tool options and tabs appear only when a document is open.

Control rows use `horizontal_wrapped`; tabs use a horizontal scroll area. There are no custom breakpoints. Global item spacing applies in both axes, and the configured interaction-height baseline is (28 pt); `small_button` is used for tab closing and error dismissal. The toolbar inserts (3 pt) between its two rows.

The canvas takes remaining space. Fit centers the image with (40 pt) total allowance in each axis and does not enlarge it beyond its native dimensions. Manual zoom ranges from (1%–1600%); panning preserves image-coordinate editing. The empty state uses centered text, an Open image action, and a supported-format line, separated by (8/16/12 pt) gaps.

## Elevation & Depth

The workspace uses neutral fills, panel dividers, and state accents. The canvas uses `Frame::NONE`; no custom editor shadows are defined. The unsaved-changes modal and standard egui popups retain their library elevation and interaction styling. There are no application-defined decorative animations; progress uses egui spinners.

## Shapes

The canvas, checkerboard, OCR boxes, and selected-image rectangles have square corners. Selection corner handles use the documented handle shape. Other button, panel, input, and modal radii are inherited from egui; the provisional seed's radius values were not implemented as Candy overrides.

## Components

### Toolbar and tool selection

Text buttons provide Open, Save, Save as, Undo, Redo, Fit, and 100%. A second row uses selectable labels for Select, Mosaic, Highlight, Text mosaic, Resize, and Flatten alpha. Selection colors come from the coral theme overrides. Hover, focus, and disabled rendering otherwise remain standard egui behavior. Actions disable when unavailable or while the document is busy.

### Document tabs

Selectable filename labels carry the coral active treatment. An asterisk indicates unsaved edits. Each label has an adjacent small Close button and separator. The native window title also reflects the filename and dirty marker.

### Contextual controls

The options row uses egui buttons, sliders, color controls, drag-value fields, and a ratio checkbox as needed by the selected tool. Image measurements are labeled in pixels. OCR exposes detection, region count, block size, Apply to all, and Remove box; regions remain visible for adjustment before applying.

### Canvas and region overlays

Transparency cells are (12 pt) squares in screen space. Ordinary selection outlines use a (1.5 pt) inside stroke. OCR regions use a (1 pt) outline, with (2 pt) for the active region, plus the accent fill described above. The selected region receives white corner handles with a (1 pt) accent border. Handle hit detection uses a (9 pt) radius around each corner. The pointer becomes a crosshair for selection/highlight or a grab cursor while Space is held.

### Status and confirmations

The bottom strip shows image dimensions, zoom, selected-region coordinates and size when applicable, progress, and the latest notice. Errors use egui's error foreground with a Dismiss action. An unsaved-changes modal has a (400 pt) maximum content width and Save copy, Discard, and Cancel actions. File selection uses native dialogs.

## Do's and Don'ts

### Do:
- Do keep the canvas as the main workspace.
- Do use coral for selected tools, selected documents, and image-region outlines.
- Do preserve system-following light and dark appearances.
- Do retain text labels, contextual options, and visible status feedback.

### Don't:
- Don't add decorative cards or dashboard composition to the editor.
- Don't treat the colors or typography inside an opened image as Candy interface tokens.
- Don't replace inherited egui styles with guessed native or web values.
