# Candy editor

## Direction

Operate mode. A familiar Preview-like image workspace, as approved in the implementation plan. The canvas takes priority over controls. Use one compact toolbar, contextual tool options, document tabs, and a quiet status strip. Empty state has an Open image action and supported formats; no dashboard or decorative cards.

## Tokens

System-following light/dark mode. Body text 14 pt, small text 12 pt, title 24 pt. Spacing scale 4/8/12/16/24 pt. Toolbar controls at least 28 pt tall. Window starts at 1120×780 pt and supports 760×520 pt. Accent is warm coral (light #B74632, dark #FF9B83), restricted to selection and active controls. Neutral canvas and a transparency checkerboard. Panel corners 6 pt; toolbar buttons 4 pt. No decorative animations.

## Interaction

Selection and brush coordinates belong to image pixels. Cmd+O/S/Z, Shift+Cmd+S/Z, Escape, Space-drag to pan, scroll/pinch zoom. A rectangle can be moved or resized via its corner handles. OCR boxes are reviewable before applying. Native file dialogs; progress, failures and unsaved changes must remain visible. Text labels take precedence over unfamiliar icons.
