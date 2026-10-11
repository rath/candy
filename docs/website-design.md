---
name: Candy website
description: A simple bilingual introduction to Candy, using native-inspired typography and a coral image stage.
colors:
  ink: "#242b35"
  muted: "#555c65"
  coral: "#f2a28e"
  paper: "#fff"
  soft: "#f4f5f6"
  line: "#dce0e3"
  accent: "#923e2c"
  get-background: "#f8e9e3"
  selection: "#f5baa9"
typography:
  display:
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'
    fontSize: "clamp(48px, 5.4vw, 76px)"
    fontWeight: 750
    lineHeight: 1.04
    letterSpacing: "-.04em"
  display-ko:
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'
    fontSize: "clamp(44px, 4.7vw, 66px)"
    fontWeight: 750
    lineHeight: 1.16
    letterSpacing: "-.04em"
  headline:
    fontSize: "clamp(28px, 3vw, 40px)"
    lineHeight: 1.15
    letterSpacing: "-.03em"
  title:
    fontSize: "18px"
    letterSpacing: "-.02em"
  intro:
    fontSize: "19px"
    lineHeight: 1.65
  body:
    fontSize: "15px"
    lineHeight: 1.7
  label:
    fontSize: "15px"
    fontWeight: 650
  command:
    fontFamily: "ui-monospace, SFMono-Regular, Consolas, monospace"
    fontSize: "16px"
    lineHeight: 2
rounded:
  control: "10px"
  build: "12px"
  stage: "20px"
spacing:
  small: "18px"
  medium: "24px"
  large: "28px"
  section-gap: "42px"
components:
  button-primary:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
    typography: "{typography.label}"
    rounded: "{rounded.control}"
    padding: "13px 22px"
  button-primary-hover:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.paper}"
  image-stage:
    backgroundColor: "{colors.coral}"
    rounded: "{rounded.stage}"
    padding: "38px 24px"
  build-block:
    backgroundColor: "{colors.paper}"
    rounded: "{rounded.build}"
    padding: "24px"
---

# Design System: Candy website

## Overview

This document describes the implemented marketing surface in `site/`, shared by English `/` and Korean `/ko/`. It extends Candy's familiar native presentation with a white page, ink typography and a coral stage around a real editor capture. It does not replace the AppKit conventions in root `DESIGN.md` or the native tokens in `.impeccable/design.json`.

The authoritative implementation is `site/assets/style.css`; the surface purpose and content contract live in `docs/website-brief.md`. Both languages use the same structure, assets and stylesheet. The site is complete without JavaScript.

## Colors

Coral gives the editor screenshot its main visual emphasis. The deeper accent colors the second headline line, hovered links, primary-button hover and keyboard focus. Ink carries headings and primary actions; muted text carries descriptions and supporting facts. White is the default canvas, soft gray groups terminal content, and a pale warm background groups installation instructions. Fine neutral rules separate features and file-format information.

The site explicitly uses a light color scheme, independently of the native app's appearance.

## Typography

Use the platform sans-serif stack without downloaded or bundled fonts. Command examples use the platform monospace stack. Headings have tight tracking and balanced wrapping; paragraphs have generous leading. Introductory copy is limited to 38 characters per line, feature copy to 35, and other prose generally to 45–50.

Korean headings and feature/introductory copy keep words together. The Korean display role has its own size and leading; its introductory copy is 18px. At the intermediate breakpoint English display text becomes 52px. At the mobile breakpoint display text is 56px in English and 50px in Korean; on the narrowest screens these become 48px and 44px. Mobile English introductory copy becomes 17px and feature copy 14px.

## Layout

The centered content width is capped at 1200px with 40px side gutters. At 950px viewport width the gutters become 24px; at 700px they become 20px. The desktop hero uses a `.86fr / 1.14fr` split with a 56px gap. Terminal and build sections use equal columns with a 72px gap. Features use three columns and top rules rather than enclosing cards.

At 950px the hero gap becomes 32px and terminal/build gaps become 40px. At 700px the hero, terminal and build sections stack; features become two columns, file-format rows stack, and the footer becomes vertical. Below 370px features become a single column. Action links wrap naturally. Code examples scroll horizontally within their own blocks.

Desktop hero padding is 64px above and 90px below, increasing to 90px and 110px at 1500px. Other major sections use 50–88px vertical padding. Mobile section padding reduces to 36–54px. The mobile header keeps language navigation and hides its extra GitHub link; the hero still provides GitHub access.

## Elevation & Depth

Most content is flat. The real editor image is the single raised object, using `0 18px 36px #57231335`. Its coral stage rotates −2 degrees and the image rotates back 2 degrees, keeping the application upright. The image arrives once over 0.7 seconds with `cubic-bezier(.16,1,.3,1)`, moving up from 12px and clearing a 2px blur. Reduced-motion preference disables this animation and smooth anchor scrolling.

## Shapes

Rounded corners identify the primary action, image, coral stage and build block. Feature articles remain open text columns with thin top borders. Do not turn the feature list into an unrelated collection of elevated cards.

## Components

- **Primary action:** an anchor styled as an ink button, with a minimum height of 48px. Its destination is the installation section. Supporting actions remain underlined text links.
- **Language navigation:** visible English and Korean links, each declaring its language. The current page is bold and marked with `aria-current="page"`.
- **Editor figure:** one real, unmodified Candy capture with intrinsic dimensions, descriptive localized alternative text and a caption identifying the synthetic sample. Asset provenance is recorded in `site/assets/credits.txt`.
- **Terminal example:** selectable command text, with decorative shell prompts excluded from selection and the accessibility tree. The Homebrew install command remains plain text inside a white inset block.
- **Page structure:** one main heading, labeled sections, native links, header, main and footer landmarks. A localized skip link appears on keyboard focus. Links receive a 3px accent focus outline offset by 5px.

## Do's and Don'ts

- **Do** preserve equivalent content and navigation in both languages, including localized alternative text and document language.
- **Do** preserve visible keyboard focus, reduced-motion handling and readable code overflow when adjusting layouts.
- **Do** show real native application imagery and retain its provenance.
- **Do** distinguish notarized public downloads from local source builds; state the Apple Silicon and macOS requirements beside installation instructions.
- **Don't** apply these website tokens to native AppKit controls or replace the native design documents.
