Installers ship with releases: a v* tag builds the Windows installer set
(NSIS and MSI), the macOS set (dmg and app), and the Linux set (deb and
AppImage) on all three platforms and attaches them to the release. The
installers are unsigned for now (Windows shows SmartScreen and macOS asks
for the right-click open); signing waits on the owner's certificates. The
tauri build config's staging chain also became live again: a duplicate key
pair had left `cargo tauri build` unable to run on a clean checkout.

Platforms: desktop
Bump: none
