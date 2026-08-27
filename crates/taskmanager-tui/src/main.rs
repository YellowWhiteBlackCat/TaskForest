//! Binary entry point: parses CLI arguments and dispatches to live, demo, or
//! headless snapshot modes exposed by `taskmanager-tui`.

#![forbid(unsafe_code)]

use std::io;

use taskmanager_assets::product;

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => taskmanager_tui::run_live(),
        Some("--demo") => taskmanager_tui::run_demo(),
        Some("--snapshot") => {
            let width = parse_dimension(args.next(), 120, "width")?;
            let height = parse_dimension(args.next(), 36, "height")?;
            print!("{}", taskmanager_tui::snapshot_text(width, height));
            Ok(())
        }
        Some("--help" | "-h") => {
            println!(
                "{} TUI — {}\n\n  taskmanager-tui              live platform telemetry\n  taskmanager-tui --demo       deterministic interactive demo\n  taskmanager-tui --snapshot [WIDTH HEIGHT]\n                                headless text-frame evidence",
                product::NAME,
                product::TAGLINE_EN
            );
            Ok(())
        }
        Some(argument) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown argument: {argument}; use --help"),
        )),
    }
}

fn parse_dimension(value: Option<String>, fallback: u16, name: &str) -> io::Result<u16> {
    let Some(value) = value else {
        return Ok(fallback);
    };
    value.parse::<u16>().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid {name} {value:?}: {error}"),
        )
    })
}
