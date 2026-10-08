# Changelog contract

User-visible changes get a fragment in unreleased/ named `<slug>.md`:

    One to three sentences describing the change from the user's view.

    Platforms: desktop
    Bump: none

`Bump` is the suggested semver movement (none, minor, major). The release PR
moves fragments to released/ under the coming version, oldest first, and is
the only place VERSION moves. Fragments are English only.
