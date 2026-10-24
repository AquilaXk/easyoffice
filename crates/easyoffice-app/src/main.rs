use easyoffice_app::{run_self_test, EasyOfficeIpcRouter};
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.iter().any(|a| a == "--self-test" || a == "--doctor") {
        match run_self_test() {
            Ok(report) => {
                println!("{report}");
                return ExitCode::SUCCESS;
            }
            Err(err) => {
                eprintln!("Self-test failed: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

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
