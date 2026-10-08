# The harness image: Stalwart plus Stalwart's own CLI, which the entrypoint configures it
# with. Stalwart v0.16 has no configuration file; a fresh server is set up through its
# management API, and the CLI is the tool that speaks it.
#
# Both pinned by multi-arch index digest (linux/amd64 + linux/arm64), carried over from
# the mail-calendar harness these pins were proven in. Bump deliberately: re-resolve the
# digest (`docker buildx imagetools inspect <image>:<tag>`, the top-level `Digest:`) and
# update the version in the tag. The CLI is a static binary, so it runs in the server's
# image as copied.
FROM ghcr.io/stalwartlabs/cli:1.0.12@sha256:fe199affac1d120a8c200ef39ae629765a2976270e0453575c1caf906ee15b52 AS cli

FROM stalwartlabs/stalwart:v0.16.24@sha256:ec011be228596e37e65f41aab17deed573859614430472f7eeb42178c50d87b7

COPY --from=cli /usr/local/bin/stalwart-cli /usr/local/bin/stalwart-cli
