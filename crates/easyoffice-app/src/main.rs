use easyoffice_app::cli_args::{parse_cli_args, CliCommand, ServePort, CLI_USAGE};
use easyoffice_app::{run_self_test, serve_webui, EasyOfficeIpcRouter};
use std::env;
use std::process::ExitCode;

/// Exit code for invalid command-line usage.
const USAGE_ERROR_EXIT: u8 = 2;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    let command = match parse_cli_args(&args) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!("Run 'easyoffice-app --help' for usage.");
            return ExitCode::from(USAGE_ERROR_EXIT);
        }
    };

    match command {
        CliCommand::Help => {
            println!("{CLI_USAGE}");
            ExitCode::SUCCESS
        }
        CliCommand::SelfTest => match run_self_test() {
            Ok(report) => {
                println!("{report}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("Self-test failed: {err}");
                ExitCode::FAILURE
            }
        },
        CliCommand::Serve { port, open_browser } => serve_webui(port, open_browser),
        CliCommand::Bootstrap => print_bootstrap(),
        CliCommand::Implicit => {
            let headless = env::var("EASYOFFICE_HEADLESS").is_ok() || env::var("CI").is_ok();
            if cfg!(target_os = "windows") && !headless {
                serve_webui(ServePort::Default, true)
            } else {
                print_bootstrap()
            }
        }
    }
}

fn print_bootstrap() -> ExitCode {
    match EasyOfficeIpcRouter::dispatch("get_workspace_bootstrap", "") {
        Ok(bootstrap) => {
            println!("EasyOffice Desktop Host Ready: {bootstrap}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Bootstrap error: {err}");
            ExitCode::FAILURE
        }
    }
}
