// SPDX-License-Identifier: MIT OR Apache-2.0
//! `liwin`: forwards its arguments to the daemon's `RunCommand` and prints
//! the reply. All parsing happens in the daemon, so the CLI and keybindings
//! share one command language.

use std::process::ExitCode;

#[zbus::proxy(
    interface = "io.github.liwin.Daemon1",
    default_service = "io.github.liwin.Daemon",
    default_path = "/io/github/liwin/Daemon"
)]
trait Daemon1 {
    fn run_command(&self, args: &[&str]) -> zbus::Result<String>;
}

const HELP: &str = "\
liwin — control the liwin tiling daemon

usage: liwin <command> [args...]

Commands are executed by liwin-daemon; run `liwin help` with the daemon
running to see what it accepts. Examples:
  liwin list-windows
  liwin focus left";

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        None | Some("-h" | "--help")
    ) {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if args.first().map(String::as_str) == Some("--version") {
        println!("liwin {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    match run(&args).await {
        Ok(out) => {
            if !out.is_empty() {
                println!("{out}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("liwin: {}", describe(&e));
            ExitCode::FAILURE
        }
    }
}

async fn run(args: &[String]) -> zbus::Result<String> {
    let conn = zbus::Connection::session().await?;
    let proxy = Daemon1Proxy::new(&conn).await?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    proxy.run_command(&args).await
}

fn describe(e: &zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(name, Some(msg), _) => {
            let short = name.as_str().rsplit('.').next().unwrap_or(name.as_str());
            if name.as_str() == "org.freedesktop.DBus.Error.ServiceUnknown" {
                "liwin-daemon is not running".to_owned()
            } else {
                format!("{short}: {msg}")
            }
        }
        other => other.to_string(),
    }
}
