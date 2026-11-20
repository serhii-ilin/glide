//! Command-line parsing for the `glide` binary (no external CLI crate).

use crate::location::resolve_location;

const KNOWN_FLAGS: &[&str] = &["--no-js", "--screenshot", "--help", "-h"];

/// Parsed subcommand for [`parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunMode {
    Help,
    Window {
        url: String,
        js_enabled: bool,
    },
    Screenshot {
        url: String,
        output: String,
        js_enabled: bool,
    },
}

pub fn usage() -> &'static str {
    "Glide, a small desktop browser.\n\n\
     Usage:\n  \
       glide [url]                    open a window (default https://example.com/)\n  \
       glide --screenshot [url] [png] headless PNG (default url / glide-screenshot.png)\n  \
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

    let js_enabled = !raw.iter().any(|a| a == "--no-js");

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

    let url = raw
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(|arg| resolve_location(arg))
        .unwrap_or_else(|| "https://example.com/".to_string());

    Ok(RunMode::Window { url, js_enabled })
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
                url: "https://example.com/".into(),
                js_enabled: true,
            }
        );
    }

    #[test]
    fn bare_host_becomes_https() {
        let mode = parse(["example.com".into()]).unwrap();
        assert_eq!(
            mode,
            RunMode::Window {
                url: "https://example.com/".into(),
                js_enabled: true,
            }
        );
    }

    #[test]
    fn no_js_flag_anywhere() {
        let mode = parse(["--no-js".into(), "example.com".into()]).unwrap();
        assert_eq!(
            mode,
            RunMode::Window {
                url: "https://example.com/".into(),
                js_enabled: false,
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
}
