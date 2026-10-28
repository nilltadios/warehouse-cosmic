# Managing remotes

Remotes uses a centered column with grouped rows. Each row shows the repository title, its installation and its remote name. A missing title falls back to the name. Disabled repositories are hidden initially; use "Show disabled" to include them. Search matches the name, title, URL or installation.

The filter icon opens Packages with that remote and installation selected. It includes both apps and runtimes and clears earlier search/runtime/end-of-life filters. The action menu contains details, copying, enable/disable and removal. Details shows the URL without adding it to every row. Changes still go through the command review and existing Flatpak authorization.

Popular remotes provides AppCenter, Flathub, Flathub beta, Fedora, GNOME Nightly and the WebKit Developer SDK. Choosing one fills the Add Remote dialog. The default installation is the current user; review the installation, name and URL before proceeding. "Add a repo file" opens the native file chooser, and "Add a custom remote" accepts a URL. These controls do not install anything until the review is confirmed.

The preset catalog is in `resources/popular-remotes.json`. It follows Warehouse's published catalog, with current redirect destinations for AppCenter and Flathub beta. Definitions come from the publishers' `.flatpakrepo` files, including their signing keys; Fedora uses its OCI registry URL. Regenerate the saved endpoint checks with `uv run --python 3.15 verification/check-popular-remotes.py`. This downloads definitions only and does not add host remotes.
