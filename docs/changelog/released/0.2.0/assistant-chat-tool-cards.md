The assistant chat shows what its tools did and how it was run. Every
tool call renders as a structured card carrying the tool's name, a
one-line argument summary, its audited status, and its output truncated
for display, and a reloaded thread paints the same cards from its stored
rows. The assistant's answers render as markdown (headings, lists, code,
emphasis, links) through a sandboxed renderer that never builds an HTML
string: raw HTML stays literal text, script never executes, and links
ride only the http, https, and mailto schemes, a mailto link opening the
prefilled composer the reading pane's own links do. Every recorded run
now stamps its pipeline version into the thread's row metadata, so a
stored thread names the pipeline that produced it.

Platforms: desktop
Bump: minor
