//! Pure command-line argument parser for the `easyoffice-app` binary.
//!
//! The parser never touches the environment, the network, or stdout so every
//! accepted and rejected argument shape can be verified by unit tests.

use std::fmt;

/// Default loopback port for the WebUI server.
pub const DEFAULT_SERVE_PORT: u16 = 8787;

/// Usage text printed by `--help` / `-h`.
pub const CLI_USAGE: &str = "\
Usage: easyoffice-app [OPTIONS]

Options:
  --serve [PORT]   Serve the WebUI on http://127.0.0.1:PORT (1-65535).
                   Without PORT the default 8787 is used and, if busy,
                   8788-8800 are tried. An explicit PORT never falls back.
  --open           Open the WebUI in the default browser (implies --serve).
  --self-test      Run the engine integrity self-test (alias: --doctor).
  --bootstrap      Print the workspace bootstrap JSON and exit (alias: --headless).
  -h, --help       Print this help and exit.

Opening documents from the command line is not supported yet.";

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

/// Fully parsed command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliCommand {
    Help,
    SelfTest,
    Serve {
        port: ServePort,
        open_browser: bool,
    },
    Bootstrap,
    /// No arguments: the caller picks WebUI or bootstrap based on platform/environment.
    Implicit,
}

/// Argument errors; all of them map to a non-zero exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    InvalidPort(String),
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
            CliError::UnknownOption(option) => write!(f, "unknown option '{option}'"),
            CliError::UnsupportedFileArgument(path) => write!(
                f,
                "opening files from the command line is not supported yet: '{path}' \
                 (start the WebUI with --serve and open the file from the workspace)"
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

/// Parses the arguments that follow the program name.
pub fn parse_cli_args<S: AsRef<str>>(args: &[S]) -> Result<CliCommand, CliError> {
    let mut self_test = false;
    let mut serve: Option<ServePort> = None;
    let mut open_browser = false;
    let mut bootstrap = false;

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
                let next = args.get(index + 1).map(|s| s.as_ref());
                match next {
                    Some(value) if !value.starts_with('-') => {
                        serve = Some(ServePort::Explicit(parse_port(value)?));
                        index += 1;
                    }
                    _ => serve = Some(ServePort::Default),
                }
            }
            _ if arg.starts_with("--serve=") => {
                if serve.is_some() {
                    return Err(CliError::DuplicateOption("--serve".to_string()));
                }
                serve = Some(ServePort::Explicit(parse_port(&arg["--serve=".len()..])?));
            }
            _ if arg.starts_with('-') && arg.len() > 1 => {
                return Err(CliError::UnknownOption(arg.to_string()));
            }
            _ => return Err(CliError::UnsupportedFileArgument(arg.to_string())),
        }
        index += 1;
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
    fn explicit_busy_port_fails_while_default_port_may_fall_back() {
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let busy_port = occupied.local_addr().unwrap().port();
        let err = crate::http_server::bind_with_fallback(busy_port, false).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
        let fallback = crate::http_server::bind_with_fallback(busy_port, true).unwrap();
        assert_ne!(fallback.local_addr().unwrap().port(), busy_port);
    }
}
