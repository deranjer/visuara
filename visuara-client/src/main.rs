use anyhow::Context;
use clap::{Parser, Subcommand};
use visuara_client::host::register_and_serve;
use visuara_common::embedded_config::EmbeddedConfig;
use visuara_common::signaling::ConnectCredential;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Launch the graphical app (default when no subcommand is given).
    Gui,
    /// Run as a host from the command line: register this machine and wait
    /// for incoming connections.
    Host {
        #[arg(long)]
        server_url: Option<String>,
        #[arg(long)]
        email: String,
        #[arg(long)]
        password: String,
        #[arg(long)]
        device_name: Option<String>,
    },
    /// Runs as a host using previously-saved credentials, with no
    /// interactive prompts — used by the per-user autostart entry created
    /// via the GUI's "enable unattended access" option.
    HostAutostart,
    /// Run as a controller from the command line: connect to a host device
    /// by ID and OTP.
    Controller {
        #[arg(long)]
        server_url: Option<String>,
        #[arg(long)]
        email: String,
        #[arg(long)]
        password: String,
        #[arg(long)]
        target_device_id: String,
        /// The device's current one-time password, or its fixed unattended
        /// password if --unattended is also passed.
        #[arg(long)]
        otp: String,
        /// Treat `--otp` as the device's fixed unattended password instead
        /// of a one-time password.
        #[arg(long)]
        unattended: bool,
    },
}

fn main() -> anyhow::Result<()> {
    // Referenced so a release build's linker can't optimize away the
    // embedded-config marker the server locates and patches by byte search.
    std::hint::black_box(&visuara_common::embedded_config::VISUARA_EMBEDDED_CONFIG_SLOT);
    let embedded = EmbeddedConfig::read_from_current_exe().ok().flatten().unwrap_or_default();

    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::Gui);

    let runtime = tokio::runtime::Runtime::new()?;

    match command {
        Command::Gui => {
            // eframe's run_native blocks the calling (main) thread and drives
            // its own event loop, so background networking runs on the tokio
            // runtime via its Handle rather than #[tokio::main].
            visuara_client::gui::run(runtime.handle().clone())
                .map_err(|e| anyhow::anyhow!("gui error: {e}"))?;
        }
        Command::Host { server_url, email, password, device_name } => {
            let server_url = server_url
                .or(embedded.server_url)
                .unwrap_or_else(|| "ws://127.0.0.1:8080/ws".to_string());
            let device_name = device_name.or(embedded.device_name).unwrap_or_else(|| "this-machine".to_string());
            runtime.block_on(async move {
                let input_sink = Box::new(visuara_agent::input::InputInjector::new()?);
                let handle = register_and_serve(
                    &server_url,
                    &email,
                    &password,
                    &device_name,
                    input_sink,
                    visuara_client::file_transfer::FileReceiver::default_destination(),
                )
                .await?;
                println!("Device ID: {}", handle.device_id);
                println!("One-time password: {}", handle.one_time_password);
                println!("Waiting for incoming connections. Press Ctrl+C to exit.");
                tokio::signal::ctrl_c().await?;
                Ok::<_, anyhow::Error>(())
            })?;
        }
        Command::HostAutostart => {
            runtime.block_on(async move {
                let creds = visuara_client::saved_credentials::SavedCredentials::load()?
                    .context("no saved credentials — enable unattended access from the app first")?;
                let input_sink = Box::new(visuara_agent::input::InputInjector::new()?);
                let handle = register_and_serve(
                    &creds.server_url,
                    &creds.email,
                    &creds.password,
                    &creds.device_name,
                    input_sink,
                    visuara_client::file_transfer::FileReceiver::default_destination(),
                )
                .await?;
                eprintln!("[visuara] unattended host running, device ID: {}", handle.device_id);
                tokio::signal::ctrl_c().await?;
                Ok::<_, anyhow::Error>(())
            })?;
        }
        Command::Controller { server_url, email, password, target_device_id, otp, unattended } => {
            let server_url = server_url
                .or(embedded.server_url)
                .unwrap_or_else(|| "ws://127.0.0.1:8080/ws".to_string());
            let credential = if unattended {
                ConnectCredential::UnattendedPassword(otp)
            } else {
                ConnectCredential::OneTimePassword(otp)
            };
            runtime.block_on(async move {
                let mut session = visuara_client::controller::connect(
                    &server_url,
                    &email,
                    &password,
                    &target_device_id,
                    credential,
                )
                .await?;
                println!("Connected. Waiting for video frames. Press Ctrl+C to exit.");
                loop {
                    tokio::select! {
                        frame = session.frames.recv() => {
                            match frame {
                                Some(f) => println!("received frame {}x{}", f.width(), f.height()),
                                None => break,
                            }
                        }
                        _ = tokio::signal::ctrl_c() => break,
                    }
                }
                Ok::<_, anyhow::Error>(())
            })?;
        }
    }
    Ok(())
}
