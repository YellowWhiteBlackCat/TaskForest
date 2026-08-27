# Font provenance and license

Bundled font files live in `assets/fonts/` and are embedded into the
TaskManager binary so the UI keeps a consistent CJK + monospace face even on
systems whose native fonts lack those glyphs. Fonts are always registered in
addition to (never replacing) the system font stack; per-skin system fonts
remain the default, and the bundled faces act as fallback or as the user's
explicit choice in Settings → Font.

## MiSans VF

- File: `fonts/MiSansVF.ttf` (variable font, family name "MiSans VF")
- Source: MiSans (小米兰亭) VF, Copyright 2020-2025 Beijing Xiaomi Mobile
  Software Co., Ltd. / Hanyi Fonts.
- License: SIL Open Font License 1.1 (`fonts/OFL-1.1.txt`). Redistribution
  requires this license to accompany the font; it is embedded in the binary
  next to the font data.

## Roboto Mono

- File: `fonts/RobotoMono-VF.ttf` (variable font, family name "Roboto Mono",
  wght axis 100–700)
- Source: Google Fonts / Roboto Mono variable font, Copyright 2015 The Roboto
  Mono Project Authors (https://github.com/googlefonts/robotomono).
- License: SIL Open Font License 1.1 (`fonts/OFL-RobotoMono.txt`).
  Redistribution requires this license to accompany the font; it is embedded
  in the binary next to the font data.
