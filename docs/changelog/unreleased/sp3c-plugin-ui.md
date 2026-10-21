# The plugin UI surfaces and the decode seam (SP3c)

message.decode lands in the reading pipeline (raw source to the serving
plugin, decoded legs through the identical sanitise pipeline, the security
banner in the reading snapshot's new leg and the PluginBanner). The manifest's
`[ui]` table declares composer actions, settings sections, and frame pages,
rendered by Astra chrome (the composer action row, the Plugins settings page)
with every interaction a router-checked custom-namespace call. Rich pages run
sandboxed in the PluginFrame over the astra-plugin:// scheme (per-plugin asset
roots, traversal refused, locked CSP) with a narrow postMessage bridge. The
outbox panel renders the send gate's held rows with answer/retry/withdraw.

Platforms: desktop
Bump: minor
