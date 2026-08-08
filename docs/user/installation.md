# Installation

Vinpst is preparing its first `0.1.0` release. Public release packages are not published yet, so files under `target/` and CI fixtures are not end-user packages.

## Release packages

The first release is being prepared for:

- Arch Linux x86_64;
- Debian 12 amd64;
- Ubuntu 24.04 amd64;
- an x86_64 Flatpak extension preview for `org.fcitx.Fcitx5//stable`.

When those artifacts are published, install the package built for your distribution.

### Arch Linux

```sh
sudo pacman -U ./fcitx-vinpst-0.1.0-1-x86_64.pkg.tar.zst
```

### Debian 12

```sh
sudo apt install ./fcitx-vinpst_0.1.0-1_debian12_amd64.deb
```

### Ubuntu 24.04

```sh
sudo apt install ./fcitx-vinpst_0.1.0-1_ubuntu24.04_amd64.deb
```

### Flatpak preview

The Flatpak build extends the Fcitx Flatpak; it does not attach to a system-installed Fcitx.

```sh
flatpak info --user org.fcitx.Fcitx5
flatpak install --user --bundle ./fcitx-vinpst-0.1.0-x86_64.flatpak
```

If you use the system Fcitx package, prefer the native Vinpst package for your distribution.

## Development checkout

Use this only when intentionally testing or developing the current source tree.

You need Rust 1.88 or newer, Cargo, CMake, a C++ compiler, Fcitx 5 development files, PipeWire development files, gettext, and `just`.

Build and install for the current user:

```sh
just build
just addon-fcitx-build
just install-user
```

The installer prints the files it writes and the commands needed to refresh the current session. Remove this per-user installation with:

```sh
just user-remove
```

## After installation

Initialize the user configuration, start the daemon, and reload Fcitx:

```sh
vinpst init
systemctl --user enable --now vinpst-daemon.service
fcitx5 -r
```

Then follow the [Quick start](quick-start.md) to install a model and try dictation.

## Uninstall

Finish any active recording and remove Vinpst through the same package manager that installed it. Package removal stops the daemon safely but keeps your configuration, downloaded models, provider/adapter scripts, hotwords, and caches.

Delete those user files manually only when you intentionally want to discard all Vinpst state.

For release integrity/provenance details and maintainer procedures, see the **Development → Publishing and rollback** documentation rather than the normal installation guide.
