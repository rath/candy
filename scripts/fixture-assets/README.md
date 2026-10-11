# Fixture source artwork

`coast.jpg` is a synthetic, fictional Mediterranean coastline, generated with
OpenAI's built-in image generation tool on 2026-10-11. It depicts no real trip,
customer, or identifiable person. It is included as a Candy demo asset under the
repository's MIT license. No third-party stock photo or remote URL is required.

The generated PNG was converted to JPEG at quality 90 with macOS `sips`.
`scripts/make-fixtures.swift` crops this committed source to fill the photo panel
and renders the typography using macOS system fonts. Regeneration needs no API
access, credentials, or downloaded fonts.

## Original generation prompt

> Use case: photorealistic-natural. Asset type: synthetic travel photograph used
> inside an image editor's reproducible demo fixture. Create a striking vertical
> editorial photograph of an imaginary Mediterranean coastal cove from a high
> cliff: vivid clear turquoise water, cobalt blue deeper sea, terracotta and pale
> peach houses clinging to a sunlit rocky headland, a small winding stone staircase,
> bougainvillea in the near foreground, one tiny white boat giving scale. Rich
> natural sunlight, tactile stone, beautiful restrained analog travel magazine
> color, crisp photographic detail. Composition: portrait 2:3, coast and warm
> architecture on the right upper side, sweeping turquoise bay through center and
> bottom left. The photograph itself must fill the entire frame with no border,
> no collage, no lettering, no signage, no logo, no watermark, no interface and no
> people. This is a fictional generated place, not a claim of documenting a real
> location.
