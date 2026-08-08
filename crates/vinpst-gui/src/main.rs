//! Vinpst Rust management GUI executable.

use std::{env, path::PathBuf};

use vinpst_gui::Page;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let mut check = false;
    let mut offline = false;
    let mut config = None::<PathBuf>;
    let mut page = Page::Control;

    while let Some(argument) = args.next() {
        match argument.to_string_lossy().as_ref() {
            "--version" | "-V" => {
                println!("vinpst-gui {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--check" => check = true,
            "--offline" => offline = true,
            "--config" => {
                let value = args.next().ok_or("--config requires a path")?;
                config = Some(PathBuf::from(value));
            }
            "--page" => {
                let value = args.next().ok_or("--page requires a page name")?;
                page = parse_page(&value.to_string_lossy())?;
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => return Err(format!("unknown argument `{other}`").into()),
        }
    }

    if offline && !check {
        return Err("--offline requires --check".into());
    }
    if config.is_some() && !check {
        return Err("--config requires --check".into());
    }
    if check {
        if page != Page::Control {
            return Err("--page cannot be combined with --check".into());
        }
        let snapshot = vinpst_gui::headless_snapshot(config.as_deref(), !offline)?;
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
        return Ok(());
    }

    vinpst_gui::run_on_page(page)?;
    Ok(())
}

fn parse_page(value: &str) -> Result<Page, Box<dyn std::error::Error>> {
    match value {
        "control" => Ok(Page::Control),
        "resources" => Ok(Page::Resources),
        "llm" => Ok(Page::Llm),
        "hotwords" => Ok(Page::Hotwords),
        other => Err(format!(
            "unknown page `{other}`; expected control, resources, llm, or hotwords"
        )
        .into()),
    }
}

fn print_help() {
    println!("Vinpst management application");
    println!();
    println!("Usage: vinpst-gui [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --page <PAGE>    Open control, resources, llm, or hotwords");
    println!("  -V, --version    Print version");
    println!("  -h, --help       Print help");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_initial_pages() {
        assert_eq!(parse_page("control").unwrap(), Page::Control);
        assert_eq!(parse_page("resources").unwrap(), Page::Resources);
        assert_eq!(parse_page("llm").unwrap(), Page::Llm);
        assert_eq!(parse_page("hotwords").unwrap(), Page::Hotwords);
        assert!(parse_page("settings").is_err());
    }
}
