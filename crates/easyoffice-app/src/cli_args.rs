//! Pure command-line argument parser for the `easyoffice-app` binary.
//!
//! The parser never touches the environment, the network, or stdout so every
//! accepted and rejected argument shape can be verified by unit tests.

use std::fmt;

/// Default loopback port for the WebUI server.
pub const DEFAULT_SERVE_PORT: u16 = 8787;

/// Upper bound on `--max-items` for `--inspect` (`PD-248`).
pub const MAX_INSPECT_ITEMS_ARG: usize = 10_000;

/// Usage text printed by `--help` / `-h`.
pub const CLI_USAGE: &str = "\
Usage: easyoffice-app [OPTIONS]

Options:
  --serve [PORT]     [Developer/testing only - not for end users / 개발·테스트 전용]
                     Serve the WebUI on http://127.0.0.1:PORT (1-65535).
                     Without PORT the default 8787 is used and, if busy,
                     8788-8800 are tried. An explicit PORT never falls back.
  --open             Open the WebUI in the default browser (implies --serve;
                     developer/testing only / 개발·테스트 전용).
  --inspect <FILE>   Print structured readback & data-check JSON for FILE and exit
                     (alias: --readback <FILE>).
  --sheet <NAME>     Target sheet name for --inspect on spreadsheet files.
  --range <A1[:B2]>  Target A1 cell/range (e.g. A1:D20 or Sheet1!A1:D20) for --inspect.
  --max-items <N>    Maximum items returned per readback collection (1-10000, default 1000).
  --self-test        Run the engine integrity self-test (alias: --doctor).
  --bootstrap        Print the workspace bootstrap JSON and exit (alias: --headless).
  -h, --help         Print this help and exit.

End users: install the EasyOffice desktop app; --serve is for development and
automated UI verification only. / 일반 사용자는 설치형 앱을 사용하세요.
Opening documents from the command line is not supported yet (use --inspect <FILE> for JSON readback).";

/// Which port the WebUI server should bind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServePort {
    /// No port given: bind 8787, falling back to 8788..=8800 when busy.
    Default,
    /// User-specified port: bind exactly this port or fail.
    Explicit(u16),
}

impl ServePort {
    pub fn port(self) -> u16 {
        match self {
            ServePort::Default => DEFAULT_SERVE_PORT,
            ServePort::Explicit(port) => port,
        }
    }

    pub fn allows_fallback(self) -> bool {
        matches!(self, ServePort::Default)
    }
}

/// Options for `--inspect <FILE>` / `--readback <FILE>` (`PD-248`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectArgs {
    pub path: String,
    pub sheet: Option<String>,
    pub range: Option<String>,
    pub max_items: Option<usize>,
}

/// Fully parsed command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    Help,
    SelfTest,
    Serve {
        port: ServePort,
        open_browser: bool,
    },
    Inspect(InspectArgs),
    Bootstrap,
    /// No arguments: the caller picks WebUI or bootstrap based on platform/environment.
    Implicit,
}

/// Argument errors; all of them map to a non-zero exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    InvalidPort(String),
    InvalidMaxItems(String),
    MissingOptionValue(String),
    InspectModifierWithoutTarget(String),
    ConflictingOptions(String),
    UnknownOption(String),
    UnsupportedFileArgument(String),
    DuplicateOption(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::InvalidPort(value) => write!(
                f,
                "invalid port '{value}' for --serve: expected a number from 1 to 65535"
            ),
            CliError::InvalidMaxItems(value) => write!(
                f,
                "invalid value '{value}' for --max-items: expected a number from 1 to {MAX_INSPECT_ITEMS_ARG}"
            ),
            CliError::MissingOptionValue(option) => {
                write!(f, "option '{option}' requires a value")
            }
            CliError::InspectModifierWithoutTarget(option) => {
                write!(f, "option '{option}' requires --inspect <FILE>")
            }
            CliError::ConflictingOptions(detail) => write!(f, "conflicting options: {detail}"),
            CliError::UnknownOption(option) => write!(f, "unknown option '{option}'"),
            CliError::UnsupportedFileArgument(path) => write!(
                f,
                "opening files from the command line is not supported yet: '{path}' \
                 (start the WebUI with --serve and open the file from the workspace, or use --inspect '{path}')"
            ),
            CliError::DuplicateOption(option) => {
                write!(f, "option '{option}' was given more than once")
            }
        }
    }
}

impl std::error::Error for CliError {}

fn parse_port(value: &str) -> Result<u16, CliError> {
    match value.parse::<u16>() {
        Ok(port) if port != 0 => Ok(port),
        _ => Err(CliError::InvalidPort(value.to_string())),
    }
}

fn parse_max_items(value: &str) -> Result<usize, CliError> {
    match value.parse::<usize>() {
        Ok(n) if (1..=MAX_INSPECT_ITEMS_ARG).contains(&n) => Ok(n),
        _ => Err(CliError::InvalidMaxItems(value.to_string())),
    }
}

fn take_option_value<S: AsRef<str>>(
    args: &[S],
    index: &mut usize,
    option_name: &str,
) -> Result<String, CliError> {
    let Some(next) = args.get(*index + 1).map(S::as_ref) else {
        return Err(CliError::MissingOptionValue(option_name.to_string()));
    };
    if next.is_empty() || (next.starts_with('-') && next.len() > 1) {
        return Err(CliError::MissingOptionValue(option_name.to_string()));
    }
    *index += 1;
    Ok(next.to_string())
}

fn take_inline_value(raw: &str, prefix: &str, option_name: &str) -> Result<String, CliError> {
    let val = &raw[prefix.len()..];
    if val.is_empty() {
        return Err(CliError::MissingOptionValue(option_name.to_string()));
    }
    Ok(val.to_string())
}

/// Parses the arguments that follow the program name.
pub fn parse_cli_args<S: AsRef<str>>(args: &[S]) -> Result<CliCommand, CliError> {
    let mut self_test = false;
    let mut serve: Option<ServePort> = None;
    let mut open_browser = false;
    let mut bootstrap = false;
    let mut inspect_path: Option<String> = None;
    let mut inspect_sheet: Option<String> = None;
    let mut inspect_range: Option<String> = None;
    let mut inspect_max_items: Option<usize> = None;

    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_ref();
        match arg {
            "-h" | "--help" => return Ok(CliCommand::Help),
            "--self-test" | "--doctor" => self_test = true,
            "--open" => open_browser = true,
            "--bootstrap" | "--headless" => bootstrap = true,
            "--serve" => {
                if serve.is_some() {
                    return Err(CliError::DuplicateOption("--serve".to_string()));
                }
                let next = args.get(index + 1).map(S::as_ref);
                match next {
                    Some(value) if !value.starts_with('-') => {
                        serve = Some(ServePort::Explicit(parse_port(value)?));
                        index += 1;
                    }
                    _ => serve = Some(ServePort::Default),
                }
            }
            "--inspect" | "--readback" => {
                if inspect_path.is_some() {
                    return Err(CliError::DuplicateOption("--inspect".to_string()));
                }
                inspect_path = Some(take_option_value(args, &mut index, arg)?);
            }
            "--sheet" => {
                if inspect_sheet.is_some() {
                    return Err(CliError::DuplicateOption("--sheet".to_string()));
                }
                inspect_sheet = Some(take_option_value(args, &mut index, "--sheet")?);
            }
            "--range" => {
                if inspect_range.is_some() {
                    return Err(CliError::DuplicateOption("--range".to_string()));
                }
                inspect_range = Some(take_option_value(args, &mut index, "--range")?);
            }
            "--max-items" => {
                if inspect_max_items.is_some() {
                    return Err(CliError::DuplicateOption("--max-items".to_string()));
                }
                let val = take_option_value(args, &mut index, "--max-items")?;
                inspect_max_items = Some(parse_max_items(&val)?);
            }
            _ if arg.starts_with("--serve=") => {
                if serve.is_some() {
                    return Err(CliError::DuplicateOption("--serve".to_string()));
                }
                serve = Some(ServePort::Explicit(parse_port(&arg["--serve=".len()..])?));
            }
            _ if arg.starts_with("--inspect=") => {
                if inspect_path.is_some() {
                    return Err(CliError::DuplicateOption("--inspect".to_string()));
                }
                inspect_path = Some(take_inline_value(arg, "--inspect=", "--inspect")?);
            }
            _ if arg.starts_with("--readback=") => {
                if inspect_path.is_some() {
                    return Err(CliError::DuplicateOption("--inspect".to_string()));
                }
                inspect_path = Some(take_inline_value(arg, "--readback=", "--readback")?);
            }
            _ if arg.starts_with("--sheet=") => {
                if inspect_sheet.is_some() {
                    return Err(CliError::DuplicateOption("--sheet".to_string()));
                }
                inspect_sheet = Some(take_inline_value(arg, "--sheet=", "--sheet")?);
            }
            _ if arg.starts_with("--range=") => {
                if inspect_range.is_some() {
                    return Err(CliError::DuplicateOption("--range".to_string()));
                }
                inspect_range = Some(take_inline_value(arg, "--range=", "--range")?);
            }
            _ if arg.starts_with("--max-items=") => {
                if inspect_max_items.is_some() {
                    return Err(CliError::DuplicateOption("--max-items".to_string()));
                }
                let val = take_inline_value(arg, "--max-items=", "--max-items")?;
                inspect_max_items = Some(parse_max_items(&val)?);
            }
            _ if arg.starts_with('-') && arg.len() > 1 => {
                return Err(CliError::UnknownOption(arg.to_string()));
            }
            _ => return Err(CliError::UnsupportedFileArgument(arg.to_string())),
        }
        index += 1;
    }

    if inspect_path.is_none() {
        if inspect_sheet.is_some() {
            return Err(CliError::InspectModifierWithoutTarget(
                "--sheet".to_string(),
            ));
        }
        if inspect_range.is_some() {
            return Err(CliError::InspectModifierWithoutTarget(
                "--range".to_string(),
            ));
        }
        if inspect_max_items.is_some() {
            return Err(CliError::InspectModifierWithoutTarget(
                "--max-items".to_string(),
            ));
        }
    }

    if let Some(path) = inspect_path {
        if self_test || serve.is_some() || open_browser || bootstrap {
            return Err(CliError::ConflictingOptions(
                "--inspect cannot be combined with --serve, --open, --self-test, or --bootstrap"
                    .to_string(),
            ));
        }
        return Ok(CliCommand::Inspect(InspectArgs {
            path,
            sheet: inspect_sheet,
            range: inspect_range,
            max_items: inspect_max_items,
        }));
    }

    if self_test {
        return Ok(CliCommand::SelfTest);
    }
    if let Some(port) = serve {
        return Ok(CliCommand::Serve { port, open_browser });
    }
    if open_browser {
        return Ok(CliCommand::Serve {
            port: ServePort::Default,
            open_browser: true,
        });
    }
    if bootstrap {
        return Ok(CliCommand::Bootstrap);
    }
    Ok(CliCommand::Implicit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<CliCommand, CliError> {
        parse_cli_args(args)
    }

    #[test]
    fn help_flags_print_usage_instead_of_bootstrap() {
        assert_eq!(parse(&["--help"]), Ok(CliCommand::Help));
        assert_eq!(parse(&["-h"]), Ok(CliCommand::Help));
        assert_eq!(parse(&["--serve", "8787", "--help"]), Ok(CliCommand::Help));
        assert!(CLI_USAGE.contains("--serve [PORT]"));
        assert!(CLI_USAGE.contains("--inspect <FILE>"));
        assert!(CLI_USAGE.contains("--self-test"));
    }

    #[test]
    fn serve_accepts_default_and_explicit_ports_with_open() {
        assert_eq!(
            parse(&["--serve", "18787"]),
            Ok(CliCommand::Serve {
                port: ServePort::Explicit(18787),
                open_browser: false
            })
        );
        assert_eq!(
            parse(&["--serve"]),
            Ok(CliCommand::Serve {
                port: ServePort::Default,
                open_browser: false
            })
        );
        assert_eq!(
            parse(&["--serve", "--open"]),
            Ok(CliCommand::Serve {
                port: ServePort::Default,
                open_browser: true
            })
        );
        assert_eq!(
            parse(&["--serve", "8787", "--open"]),
            Ok(CliCommand::Serve {
                port: ServePort::Explicit(8787),
                open_browser: true
            })
        );
        assert_eq!(
            parse(&["--open", "--serve=9000"]),
            Ok(CliCommand::Serve {
                port: ServePort::Explicit(9000),
                open_browser: true
            })
        );
        assert_eq!(
            parse(&["--open"]),
            Ok(CliCommand::Serve {
                port: ServePort::Default,
                open_browser: true
            })
        );
        assert!(ServePort::Default.allows_fallback());
        assert_eq!(ServePort::Default.port(), DEFAULT_SERVE_PORT);
        assert!(!ServePort::Explicit(8787).allows_fallback());
    }

    #[test]
    fn invalid_ports_are_rejected_instead_of_silently_replaced() {
        for bad in ["abc", "99999", "0", "65536", "-1x", "80a"] {
            let result = if bad.starts_with('-') {
                parse(&[&format!("--serve={bad}")])
            } else {
                parse(&["--serve", bad])
            };
            assert_eq!(result, Err(CliError::InvalidPort(bad.to_string())), "{bad}");
        }
        assert_eq!(
            parse(&["--serve=abc"]),
            Err(CliError::InvalidPort("abc".to_string()))
        );
        assert_eq!(
            parse(&["--serve", "1", "--serve"]),
            Err(CliError::DuplicateOption("--serve".to_string()))
        );
    }

    #[test]
    fn unknown_options_and_file_paths_are_errors() {
        assert_eq!(
            parse(&["--frobnicate"]),
            Err(CliError::UnknownOption("--frobnicate".to_string()))
        );
        assert_eq!(
            parse(&["-x"]),
            Err(CliError::UnknownOption("-x".to_string()))
        );
        let path = r"C:\Users\me\Documents\report.hwp";
        let err = parse(&[path]).unwrap_err();
        assert_eq!(err, CliError::UnsupportedFileArgument(path.to_string()));
        assert!(err.to_string().contains("not supported yet"));
        assert!(matches!(
            parse(&["--serve", "8787", "extra"]),
            Err(CliError::UnsupportedFileArgument(_))
        ));
        assert!(CliError::InvalidPort("abc".into())
            .to_string()
            .contains("1 to 65535"));
    }

    #[test]
    fn existing_modes_keep_working() {
        assert_eq!(parse(&["--self-test"]), Ok(CliCommand::SelfTest));
        assert_eq!(parse(&["--doctor"]), Ok(CliCommand::SelfTest));
        assert_eq!(parse(&["--bootstrap"]), Ok(CliCommand::Bootstrap));
        assert_eq!(parse(&["--headless"]), Ok(CliCommand::Bootstrap));
        assert_eq!(
            parse(&["--headless", "--serve", "18787"]),
            Ok(CliCommand::Serve {
                port: ServePort::Explicit(18787),
                open_browser: false
            })
        );
        let empty: [&str; 0] = [];
        assert_eq!(parse_cli_args(&empty), Ok(CliCommand::Implicit));
    }

    #[test]
    fn inspect_and_readback_flags_parse_and_reject_invalid_combinations() {
        assert_eq!(
            parse(&[
                "--inspect",
                "budget.xlsx",
                "--sheet",
                "Summary",
                "--range",
                "A1:D10",
                "--max-items",
                "250"
            ]),
            Ok(CliCommand::Inspect(InspectArgs {
                path: "budget.xlsx".to_string(),
                sheet: Some("Summary".to_string()),
                range: Some("A1:D10".to_string()),
                max_items: Some(250),
            }))
        );
        assert_eq!(
            parse(&["--readback=report.hwpx", "--max-items=50"]),
            Ok(CliCommand::Inspect(InspectArgs {
                path: "report.hwpx".to_string(),
                sheet: None,
                range: None,
                max_items: Some(50),
            }))
        );
        assert_eq!(
            parse(&["--sheet", "S1"]),
            Err(CliError::InspectModifierWithoutTarget(
                "--sheet".to_string()
            ))
        );
        assert_eq!(
            parse(&["--inspect"]),
            Err(CliError::MissingOptionValue("--inspect".to_string()))
        );
        assert_eq!(
            parse(&["--inspect", "a.xlsx", "--max-items", "0"]),
            Err(CliError::InvalidMaxItems("0".to_string()))
        );
        assert_eq!(
            parse(&["--inspect", "a.xlsx", "--max-items", "10001"]),
            Err(CliError::InvalidMaxItems("10001".to_string()))
        );
        assert!(matches!(
            parse(&["--inspect", "a.xlsx", "--serve"]),
            Err(CliError::ConflictingOptions(_))
        ));
    }

    #[test]
    fn explicit_busy_port_fails_while_default_port_may_fall_back() {
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let busy_port = occupied.local_addr().unwrap().port();
        let err = crate::http_server::bind_with_fallback(busy_port, false).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
        let fallback = crate::http_server::bind_with_fallback(busy_port, true).unwrap();
        assert_ne!(fallback.local_addr().unwrap().port(), busy_port);
    }
}
