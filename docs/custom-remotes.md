# Custom distribution channels

Warehouse COSMIC uses Flatpak's installed origin and installation scope. It does not assume that an application comes from Flathub. Signature checks, runtime installation, extra-data downloads, checksum checks, and `apply_extra` hooks remain Flatpak's responsibility.

## Parsec autobuild

The reference project is `/home/nilllinux22/Documents/parsec-flatpak-autobuild`. Its publisher creates a signed OSTree repository on GitHub Pages:

- App: `com.parsecgaming.parsec`
- Ref: `app/com.parsecgaming.parsec/x86_64/stable`
- Remote: `parsec-autobuild`
- Repository: `https://nilltadios.github.io/com.parsecgaming.parsec/repo/`
- Add-remote file: `https://nilltadios.github.io/com.parsecgaming.parsec/parsec.flatpakrepo`
- Install file: `https://nilltadios.github.io/com.parsecgaming.parsec/com.parsecgaming.parsec.flatpakref`

The install file embeds the public signing key and points `RuntimeRepo` at Flathub. The installed package currently uses Freedesktop 26.08. This comes from the CI override; the base manifest alone is not the published runtime specification. Parsec's real build number is read as a string, including values such as `150-104a`.

The graphical app can open the downloaded install file, add its repo file, search `parsec-autobuild`, check for an update, update from the installed origin, or change an existing installation from another source. "Change Source" reinstalls the same full ref from a chosen remote in the same installation. It keeps `~/.var/app/com.parsecgaming.parsec`.

Read and plan through the standalone CLI:

```sh
warehouse-cosmic-cli run --cmd 'package.update-check={"installation":"user","reference":"app/com.parsecgaming.parsec/x86_64/stable"}'
warehouse-cosmic-cli run --cmd 'package.update={"installation":"user","references":["app/com.parsecgaming.parsec/x86_64/stable"]}'
warehouse-cosmic-cli run --cmd 'package.reinstall={"installation":"user","reference":"app/com.parsecgaming.parsec/x86_64/stable","remote":"parsec-autobuild"}'
```

The last two commands are plans. `--apply` performs the requested action. They are separate operations: updating uses the current origin, while reinstalling can change it. Package masks prevent normal updates until explicitly removed.

This publisher creates a fresh repository with one commit for each build. An older commit can disappear from the remote, so version rollback is available only when the remote still serves the requested commit and its required extra data. Warehouse COSMIC shows the history returned by Flatpak; it does not invent an archive of unpublished or pruned versions. A data snapshot backs up settings and login data, not the application's package files or its remote history.

The manager's Wayland renderer does not change the managed app's display permissions. Parsec can keep its existing X11 configuration while Warehouse COSMIC runs on Wayland.

## Verification

`verification/check-parsec.sh` fetches the public install/repo files and publication state, records their URLs and retrieval time, checks the real installed package and advertised commit, and makes dry-run plans. It checks that the publishing project's files and Git status remain unchanged. It does not apply an install, update, or source switch to the real Parsec app.

`verification/backend-smoke.sh` tests a real source switch between two signed local remotes in an isolated user installation, reads back the new origin, checks app-data preservation, switches back, and tests commit-based update detection and application. `verification/verify-cli.sh` also exercises restoration and Trash through the headless binary.

A fresh end-to-end Parsec extra-data installation is not performed against the real account. Its download and bootstrap behavior are delegated to the installed Flatpak engine, as they are for the official Warehouse frontend.
