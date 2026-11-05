# Launchers and icons that override an app's own

Desktops look in your own data folder (`$XDG_DATA_HOME`, normally `~/.local/share`) before the folder where Flatpak exports app launchers and icons. A file there with an app's ID as its name wins over the Flatpak's copy:

- `~/.local/share/applications/APP_ID.desktop` replaces the app's launcher.
- `~/.local/share/icons/THEME/SIZE/apps/APP_ID.{svg,png}` (and `APP_ID-symbolic.svg`) replaces its icon.

Such files are left behind by an earlier native or `cargo install` of the app, a menu editor, a script, or a manual fix. Warehouse only runs `flatpak`, so these copies stay in effect after an install, uninstall, reinstall or change of package source. Two cases seen on one machine ([issue #1](https://github.com/nilltadios/warehouse-cosmic/issues/1)):

- An old launcher still ran `flatpak run --branch=master …` after the app moved to a `stable` remote, so opening it from the menu failed with `app/…/master not installed`.
- An icon from a pre-Flatpak install hid the icon the Flatpak ships.

## What Warehouse COSMIC shows

The app checks these files for installed apps, for apps with leftover data, and for apps an operation just removed. Symbolic links into Flatpak's own exports are Flatpak's files and are not reported. Launchers named after other apps that do not run Flatpak are not judged.

A launcher is **broken** when it runs `flatpak run` with a branch, architecture, installation or app ID that is not installed, does not run the Flatpak at all, or belongs to an app that is no longer installed.

The app prompts in two situations, with one panel for everything it found:

- **After an install, update, version change, source change or uninstall in Warehouse COSMIC**: broken launchers and custom icons of exactly the apps that operation changed. Installs from `.flatpak` and `.flatpakref` files are covered by comparing the package list before and after, so Uninstall followed by dropping a bundle and Install asks twice: once about the leftover, once about the launcher that now breaks the new install.
- **When the app starts or the list is refreshed**: broken launchers of installed apps, wherever they came from (an earlier version of this app, `flatpak` in a terminal, another store, a script). Custom icons and working launchers do not prompt here, because they are often deliberate.

Each file in the panel has these choices:

| Choice | Effect | Asked again |
|---|---|---|
| **Fix It** (launcher), **Use App's Icon** (icon), **Move to Trash** (uninstalled app) | Moves the file to the desktop Trash, where it can be restored, then refreshes the menu cache (`update-desktop-database`) or the icon theme cache (`gtk-update-icon-cache`) when that folder already has one | No |
| **✕ Not Now** | Hides the panel | After the next operation on that app, or the next time the app starts |
| **Don't Ask Again** | Remembers the file in `kept-overrides.json` in the app's data folder | If the file changes, or the app is installed again after an uninstall (a launcher left over from an uninstalled app can break the new install) |

Whatever was chosen, the fix stays available without prompting:

- **On an app's page**, a Launcher and Icon section lists every override of that app, working launchers as a quiet note, each with its fix button. Files kept with Don't Ask Again show **Ask Again**.
- **In the Packages list**, an app with a broken launcher that was not kept shows a warning mark and "Launcher broken".
- **Under User Data → Leftover Data**, launchers and icons of uninstalled apps are listed with the leftover folders.

Nothing is removed without a click. Open windows keep the icon the compositor read when they were mapped, so the taskbar shows the new icon after the app is reopened.

## CLI

```sh
warehouse-cosmic-cli run --cmd 'override.list={"state":"broken"}'
warehouse-cosmic-cli run --cmd 'override.trash={"paths":["/home/USER/.local/share/applications/APP_ID.desktop"]}'           # plan
warehouse-cosmic-cli run --apply --cmd 'override.trash={"paths":["/home/USER/.local/share/applications/APP_ID.desktop"]}'
warehouse-cosmic-cli run --apply --cmd 'override.keep={"paths":["/home/USER/.local/share/icons/hicolor/scalable/apps/APP_ID.svg"],"keep":true}'
```

`override.list` changes nothing; `state` is `all`, `broken` or `shadowing`, and `ids` limits it to some apps. `override.trash` and `override.keep` accept only paths `override.list` currently reports. In the GUI the same buttons are `ui` controls named `overrides.N.use` / `.keep` and `overrides.dismiss` (Not Now) in the panel, `package.override.N.*` on the app page and `data.override.N.*` under Leftover Data; `use` and `keep` need `--apply`. `ui inspect` reports `override_alerts`, and each package's `launcher_broken`.

The KDE Plasma menu notices changes in `~/.local/share/applications` by itself. Warehouse does not run desktop-specific tools such as `kbuildsycoca6`.
