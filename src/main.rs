mod app;
mod descriptor;
mod device;
mod input;
mod monitor;
mod permissions;
mod protocol;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    version,
    about = "Native control for Razer Orbweaver (1532:0113), Orbweaver Chroma (1532:0207), and Tartarus V2 (1532:022b)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Open the native configuration app.
    Gui,
    #[command(hide = true)]
    CaptureHelper {
        #[arg(long)]
        socket: std::path::PathBuf,
    },
    /// Check macOS HID permissions; optionally ask macOS to request Input Monitoring.
    Permissions {
        #[arg(long)]
        request: bool,
    },
    /// List only connected supported-keypad HID interfaces; does not open them.
    Devices,
    /// Open shared handles and inspect report descriptors without changing settings.
    Inspect,
    /// Record timestamped raw reports and decoded transitions. Does not remap input.
    Monitor {
        /// Duration in seconds; zero runs until Ctrl-C.
        #[arg(long, default_value_t = 30)]
        seconds: u64,
        /// Limit to one USB interface (0, 1, or 2).
        #[arg(long, value_parser=clap::value_parser!(i32).range(0..=2))]
        interface: Option<i32>,
        /// Save JSON Lines to a new file (never overwrites).
        #[arg(long)]
        output: Option<std::path::PathBuf>,
        /// Annotate a physical-control trial, e.g. key_01 or thumb_north.
        #[arg(long)]
        label: Option<String>,
        /// macOS: temporarily seize only the selected keypad interfaces.
        #[arg(long)]
        exclusive: bool,
    },
    /// Query firmware identity through one interface; does not change settings.
    Probe {
        #[arg(long, default_value_t=2, value_parser=clap::value_parser!(i32).range(0..=2))]
        interface: i32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::Gui);
    if matches!(command, Command::Gui) {
        return app::run();
    }
    if let Command::CaptureHelper { socket } = command {
        return app::session::helper(&socket);
    }
    if let Command::Permissions { request } = command {
        println!(
            "{}",
            serde_json::to_string_pretty(&permissions::status(request))?
        );
        return Ok(());
    }
    let api = hidapi::HidApi::new()?;
    match command {
        Command::Gui | Command::CaptureHelper { .. } | Command::Permissions { .. } => {
            unreachable!()
        }
        Command::Devices | Command::Inspect => {
            let inspect = matches!(command, Command::Inspect);
            let devices = device::inventory(&api, inspect);
            println!("{}", serde_json::to_string_pretty(&devices)?);
            anyhow::ensure!(
                !devices.is_empty(),
                "No supported keypad ({}) is connected",
                device::SUPPORTED_LIST
            );
            if inspect {
                anyhow::ensure!(
                    devices
                        .iter()
                        .all(|d| d.access.as_deref() == Some("opened_shared")),
                    "inspection incomplete; see per-interface access results above"
                );
            }
        }
        Command::Monitor {
            seconds,
            interface,
            output,
            label,
            exclusive,
        } => monitor::run(
            api,
            monitor::Options {
                seconds,
                interface,
                output: output.as_deref(),
                label: label.as_deref(),
                exclusive,
            },
        )?,
        Command::Probe { interface } => {
            let model = device::connected_model(&api)?;
            anyhow::ensure!(
                model.supports_rgb(),
                "Firmware probing is verified only for RGB models; original Orbweaver key mapping does not require it"
            );
            let info = device::select(&api, interface)?;
            let handle = info.open_device(&api)?;
            let firmware = protocol::query_firmware_for(&handle, model)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"path":info.path().to_string_lossy(),"interface":interface,"firmware":firmware})
                )?
            );
        }
    }
    Ok(())
}
