# Bundled UI font

Platforms: desktop
Bump: minor

The webview now vendors Inter Variable (latin subset, a single 48KB woff2
under the OFL) and leads `--font-ui` with it, so the 380/450/560 weight
stops resolve on every platform offline instead of only where Segoe UI
Variable Text ships. CJK text continues on the system stack. A new
`font-budget` gate check keeps every vendored woff2 under 400KB.
