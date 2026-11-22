//! Command-line parsing for the `glide` binary (no external CLI crate).

use std::path::PathBuf;

use crate::location::resolve_location;

const KNOWN_FLAGS: &[&str] = &[
    "--no-js",
    "--screenshot",
    "--dump-text",
    "--mcp",
    "--attach",
    "--help",
    "-h",
];

/// Parsed subcommand for [`parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunMode {
    Help,
    Window {
        /// `None` when the user did not pass a URL (restore session).
        cli_url: Option<String>,
        js_enabled: bool,
        cli_no_js: bool,
    },
    Screenshot {
        url: String,
        output: String,
        js_enabled: bool,
    },
    DumpText {
        url: String,
        js_enabled: bool,
    },
    Mcp {
        js_enabled: bool,
        attach: Option<Option<PathBuf>>,
    },
}

pub fn usage() -> &'static str {
    "Glide, a small desktop browser.\n\n\
     Usage:\n  \
       glide [url]                    open a window (default https://example.com/)\n  \
       glide --screenshot [url] [png] headless PNG (default url / glide-screenshot.png)\n  \
       glide --dump-text [url]        print page text after scripts settle\n  \
       glide --mcp [--no-js]          MCP server on stdio (--mcp must be first)\n  \
       glide --no-js ...              disable page JavaScript\n\n\
     Options:\n  \
       -h, --help                     print this help\n"
}

pub fn parse<I>(args: I) -> Result<RunMode, String>
where
    I: IntoIterator<Item = String>,
{
    let raw: Vec<String> = args.into_iter().collect();

    if raw.iter().any(|a| a == "--help" || a == "-h") {
        return Ok(RunMode::Help);
    }

    for arg in &raw {
        if arg.starts_with("--") && !KNOWN_FLAGS.contains(&arg.as_str()) {
            return Err(format!("unknown flag: {arg}"));
        }
    }

    let cli_no_js = raw.iter().any(|a| a == "--no-js");
    let js_enabled = !cli_no_js;

    if raw.first().is_some_and(|a| a == "--mcp") {
        let mut attach = None;
        let mut i = 1;
        while i < raw.len() {
            match raw[i].as_str() {
                "--no-js" => {}
                "--attach" => {
                    attach = Some(None);
                    if let Some(next) = raw.get(i + 1)
                        && !next.starts_with("--")
                    {
                        attach = Some(Some(PathBuf::from(next)));
                        i += 1;
                    }
                }
                other if other.starts_with("--") => {
                    return Err(format!("unknown flag: {other}"));
                }
                other => return Err(format!("unexpected argument for --mcp: {other}")),
            }
            i += 1;
        }
        return Ok(RunMode::Mcp { js_enabled, attach });
    }

    if raw.first().is_some_and(|a| a == "--dump-text") {
        let rest: Vec<String> = raw
            .iter()
            .skip(1)
            .filter(|a| *a != "--no-js")
            .cloned()
            .collect();
        let url = rest
            .first()
            .cloned()
            .unwrap_or_else(|| "https://example.com".into());
        return Ok(RunMode::DumpText {
            url: resolve_location(&url),
            js_enabled,
        });
    }

    if raw.first().is_some_and(|a| a == "--screenshot") {
        let rest: Vec<String> = raw
            .iter()
            .skip(1)
            .filter(|a| *a != "--no-js")
            .cloned()
            .collect();
        let url = rest
            .first()
            .cloned()
            .unwrap_or_else(|| "https://example.com".into());
        let output = rest
            .get(1)
            .cloned()
            .unwrap_or_else(|| "glide-screenshot.png".into());
        return Ok(RunMode::Screenshot {
            url: resolve_location(&url),
            output,
            js_enabled,
        });
    }

    let cli_url = raw
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(|arg| resolve_location(arg));

    Ok(RunMode::Window {
        cli_url,
        js_enabled,
        cli_no_js,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_opens_default_page() {
        let mode = parse(std::iter::empty::<String>()).unwrap();
        assert_eq!(
            mode,
            RunMode::Window {
                cli_url: None,
                js_enabled: true,
                cli_no_js: false,
            }
        );
    }

    #[test]
    fn bare_host_becomes_https() {
        let mode = parse(["example.com".into()]).unwrap();
        assert_eq!(
            mode,
            RunMode::Window {
                cli_url: Some("https://example.com/".into()),
                js_enabled: true,
                cli_no_js: false,
            }
        );
    }

    #[test]
    fn no_js_flag_anywhere() {
        let mode = parse(["--no-js".into(), "example.com".into()]).unwrap();
        assert_eq!(
            mode,
            RunMode::Window {
                cli_url: Some("https://example.com/".into()),
                js_enabled: false,
                cli_no_js: true,
            }
        );
        let mode = parse([
            "--screenshot".into(),
            "--no-js".into(),
            "https://x.test/".into(),
        ])
        .unwrap();
        assert!(matches!(
            mode,
            RunMode::Screenshot {
                js_enabled: false,
                ..
            }
        ));
    }

    #[test]
    fn screenshot_with_url_and_path() {
        let mode = parse([
            "--screenshot".into(),
            "https://a.test/page".into(),
            "/tmp/out.png".into(),
        ])
        .unwrap();
        assert_eq!(
            mode,
            RunMode::Screenshot {
                url: "https://a.test/page".into(),
                output: "/tmp/out.png".into(),
                js_enabled: true,
            }
        );
        let mode = parse([
            "--screenshot".into(),
            "https://a.test/page".into(),
            "--no-js".into(),
            "/tmp/out-nojs.png".into(),
        ])
        .unwrap();
        assert_eq!(
            mode,
            RunMode::Screenshot {
                url: "https://a.test/page".into(),
                output: "/tmp/out-nojs.png".into(),
                js_enabled: false,
            }
        );
    }

    #[test]
    fn screenshot_defaults() {
        let mode = parse(["--screenshot".into()]).unwrap();
        assert_eq!(
            mode,
            RunMode::Screenshot {
                url: "https://example.com/".into(),
                output: "glide-screenshot.png".into(),
                js_enabled: true,
            }
        );
    }

    #[test]
    fn unknown_flag_is_error() {
        let err = parse(["--nope".into()]).unwrap_err();
        assert!(err.contains("--nope"));
    }

    #[test]
    fn help_flag() {
        assert!(matches!(parse(["--help".into()]), Ok(RunMode::Help)));
        assert!(matches!(parse(["-h".into()]), Ok(RunMode::Help)));
    }

    #[test]
    fn dump_text_with_url() {
        let mode = parse(["--dump-text".into(), "example.com".into()]).unwrap();
        assert_eq!(
            mode,
            RunMode::DumpText {
                url: "https://example.com/".into(),
                js_enabled: true,
            }
        );
    }

    #[test]
    fn dump_text_with_no_js() {
        let mode = parse([
            "--dump-text".into(),
            "--no-js".into(),
            "https://x.test/".into(),
        ])
        .unwrap();
        assert_eq!(
            mode,
            RunMode::DumpText {
                url: "https://x.test/".into(),
                js_enabled: false,
            }
        );
    }

    #[test]
    fn mcp_flag_parses_headless() {
        let mode = parse(["--mcp".into()]).unwrap();
        assert_eq!(
            mode,
            RunMode::Mcp {
                js_enabled: true,
                attach: None,
            }
        );
    }

    #[test]
    fn mcp_no_js_flag() {
        let mode = parse(["--mcp".into(), "--no-js".into()]).unwrap();
        assert_eq!(
            mode,
            RunMode::Mcp {
                js_enabled: false,
                attach: None,
            }
        );
    }

    #[test]
    fn mcp_help_flag() {
        assert!(matches!(
            parse(["--mcp".into(), "--help".into()]),
            Ok(RunMode::Help)
        ));
        assert!(matches!(
            parse(["--mcp".into(), "-h".into()]),
            Ok(RunMode::Help)
        ));
    }

    #[test]
    fn mcp_attach_with_and_without_path() {
        let mode = parse(["--mcp".into(), "--attach".into()]).unwrap();
        assert_eq!(
            mode,
            RunMode::Mcp {
                js_enabled: true,
                attach: Some(None),
            }
        );
        let mode = parse(["--mcp".into(), "--attach".into(), "/tmp/glide.sock".into()]).unwrap();
        assert_eq!(
            mode,
            RunMode::Mcp {
                js_enabled: true,
                attach: Some(Some(PathBuf::from("/tmp/glide.sock"))),
            }
        );
    }
}
