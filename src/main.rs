// SPDX-License-Identifier: GPL-3.0-or-later
mod app;
mod preferences;
mod remotes;
mod smoke;
mod ui_bridge;
mod view_state;
mod window_state;
use warehouse_cosmic::{backend, snapshots};
fn main() -> cosmic::iced::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let mut flags = app::Flags::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "Warehouse COSMIC\n\nwarehouse-cosmic [FILE.flatpak|FILE.flatpakref|FILE.flatpakrepo|FILE.tar.zst]\n  --inspect                Print a read-only inventory as JSON\n  --preview PAGE           Open a view for inspection\n  --capture FILE.png       Save the app's own render and exit\n  --smoke ROOT             Run the isolated backend fixture test\n  --help                   Show this help\n\nAgents and scripts: the warehouse-cosmic-cli command manages Flatpak without a window and\ncontrols this window (ui inspect / ui activate). Start with:\n  warehouse-cosmic-cli guide\n  flatpak run --command=warehouse-cosmic-cli dev.nilltadios.WarehouseCosmic guide"
                );
                return Ok(());
            }
            "--inspect" => {
                match backend::Paths::discover().and_then(|paths| backend::load(&paths)) {
                    Ok(inventory) => println!(
                        "{}",
                        serde_json::to_string_pretty(&inventory).expect("Inventory serializes")
                    ),
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                }
                return Ok(());
            }
            "--smoke" => {
                let root = args.next().expect("--smoke requires a fixture directory");
                match smoke::run(std::path::Path::new(&root)) {
                    Ok(()) => println!("PASS: isolated Flatpak operations and snapshot round trip"),
                    Err(e) => {
                        eprintln!("FAIL: {e}");
                        std::process::exit(1);
                    }
                }
                return Ok(());
            }
            "--preview" => flags.preview = Some(args.next().expect("--preview requires a page")),
            "--capture" => {
                flags.capture = Some(args.next().expect("--capture requires a PNG path").into())
            }
            value if !value.starts_with('-') => {
                flags
                    .open_files
                    .push(std::path::absolute(value).expect("Cannot resolve file path"));
            }
            _ => {
                eprintln!("Unknown argument {arg}. See --help.");
                std::process::exit(2);
            }
        }
    }
    let mut state = if flags.capture.is_some() && flags.preview.as_deref() != Some("window-restore")
    {
        window_state::WindowState::default()
    } else {
        window_state::load().unwrap_or_else(|error| {
            eprintln!("{error}");
            window_state::WindowState::default()
        })
    };
    if flags.capture.is_some()
        && flags
            .preview
            .as_deref()
            .is_some_and(|p| p.ends_with("-narrow"))
    {
        state.width = 940;
    }
    let size = state.size();
    flags.window_state = state;
    flags.view_state = if flags.capture.is_some() {
        view_state::ViewState::default()
    } else {
        view_state::load().unwrap_or_else(|error| {
            eprintln!("{error}");
            view_state::ViewState::default()
        })
    };
    flags.activation_action = (!flags.open_files.is_empty()).then(|| "open-files".into());
    let isolated = flags.capture.is_some()
        || std::env::var("WAREHOUSE_COSMIC_UI_ISOLATED").is_ok_and(|value| value == "1");
    let settings = cosmic::app::Settings::default()
        .theme(preferences::load().unwrap_or_default().theme.theme())
        .size(size)
        .size_limits(cosmic::iced::Limits::NONE.min_width(940.).min_height(560.))
        .default_icon_theme("WarehouseCosmic");
    if isolated {
        cosmic::app::run::<app::App>(settings, flags)
    } else {
        cosmic::app::run_single_instance::<app::App>(settings, flags)
    }
}
