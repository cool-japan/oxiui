# oxiui-tray — System-tray adapter for OxiUI (quarantine crate)

[![Crates.io](https://img.shields.io/crates/v/oxiui-tray.svg)](https://crates.io/crates/oxiui-tray)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-tray` is the system-tray / menu-bar icon adapter for the COOLJAPAN OxiUI toolkit. It wraps [`tray-icon`](https://crates.io/crates/tray-icon) behind a small `TrayConfig` / `TrayMenuItem` / `TrayHandle` API for mounting a tray icon with a tooltip and a context menu.

## Pure-Rust quarantine notice

This is a standalone **COOLJAPAN Pure-Rust Policy §5 quarantine crate**, not a module of the `oxiui` facade. Enabling its `tray` feature pulls in `tray-icon`, which on Linux drags in the entire GTK/GLib C stack (`gtk-sys`, `gdk-sys`, `gdk-pixbuf-sys`, `gio-sys`, `glib-sys`, `gobject-sys`, `atk-sys`, `cairo-sys-rs`, `pango-sys`, `libappindicator-sys`) plus `dirs-sys` — system tray support has no pure path on Linux because it requires GTK at the OS layer. Per policy §5 this impurity is quarantined here: the `oxiui` facade does **not** depend on `oxiui-tray`, so apps that don't need a tray icon stay 100% Pure Rust. Apps that *do* want a system tray icon depend on this crate directly and enable its `tray` feature.

With `default = []`, the crate is inert (no `tray-icon` dependency at all) until `tray` is enabled.

## Installation

```toml
[dependencies]
# Inert by default — no tray-icon / GTK dependency:
oxiui-tray = "0.2.1"

# Enable the actual system tray icon (pulls GTK on Linux):
oxiui-tray = { version = "0.2.1", features = ["tray"] }
```

## Quick Start

```rust,no_run
use oxiui_tray::{TrayConfig, TrayMenuItem, TrayHandle};

let _handle = TrayHandle::mount(
    TrayConfig::new()
        .tooltip("My OxiUI App")
        .menu_item(TrayMenuItem::action("Show", || {}))
        .menu_item(TrayMenuItem::action("Quit", || std::process::exit(0))),
).expect("tray init failed");
```

`TrayHandle::mount` returns a live handle; dropping it removes the tray icon. Without the `tray` feature, `mount` always succeeds and returns a no-op handle, so calling code can be written once and stay portable to non-desktop targets.

## API Overview

| Item | Description |
|------|-------------|
| `TrayConfig` | Builder: `new`, `tooltip`, `icon_path`, `icon_bytes`, `menu_item`, `has_menu`. Public fields `icon_path`, `icon_bytes`, `tooltip`, `menu_items`. Derives `Debug`, `Default`, `Clone`. |
| `TrayMenuItem` | `Action { label, shortcut }`, `Separator`, `SubMenu { label, children }`. Constructors: `action`, `action_with_shortcut`, `separator`, `sub_menu`. |
| `TrayHandle` | `mount(config: TrayConfig) -> Result<Self, String>`, `set_tooltip(&self, tip: &str) -> Result<(), String>`. |

`TrayMenuItem::action` / `action_with_shortcut` accept a callback closure but do not store it on the item itself — only the label and shortcut are kept at the data-model level (for Pure-Rust serialization purposes); callbacks are wired by the backend when it processes the tray config. `TrayHandle` is a documented work in progress: full event-loop integration (tray click / menu-selection callbacks firing during the host event loop) is planned for a future release.

## Feature Flags

| Feature | Default | Effect |
|---------|---------|--------|
| `tray` | off | Enables the optional `tray-icon` dependency and builds a real OS tray icon in `TrayHandle::mount`. Pulls GTK/GLib on Linux (see quarantine notice above). |

## Related crates

- [`oxiui`](https://crates.io/crates/oxiui) — the OxiUI facade (intentionally does not depend on this crate)
- [`oxiui-core`](https://crates.io/crates/oxiui-core) — shared `UiCtx` / `Theme` types

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
