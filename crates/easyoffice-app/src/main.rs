use easyoffice_app::{run_self_test, serve_webui, EasyOfficeIpcRouter};
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

    let open_flag = args.iter().any(|a| a == "--open");
    if let Some(idx) = args.iter().position(|a| a == "--serve") {
        let port = args
            .get(idx + 1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(8787);
        return serve_webui(port, open_flag);
    }
    if open_flag {
        return serve_webui(8787, true);
    }

    let headless = args.iter().any(|a| a == "--bootstrap" || a == "--headless")
        || env::var("EASYOFFICE_HEADLESS").is_ok()
        || env::var("CI").is_ok();
    if cfg!(target_os = "windows") && args.is_empty() && !headless {
        return serve_webui(8787, true);
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
