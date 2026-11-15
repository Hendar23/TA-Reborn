//! The trust boundary for addresses supplied by feeds and release metadata.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use ta_ui::i18n::{t, tf};

pub const METADATA_LIMIT: usize = 2 << 20;

#[derive(Clone, Copy)]
pub enum Policy {
    Web,
    /// Local files are only available to an explicitly configured local update source.
    Update { local_files: bool },
}

pub fn address(raw: &str, policy: Policy) -> Result<String, String> {
    let raw = raw.trim();
    if raw.len() > 8192 || raw.chars().any(char::is_control) {
        return Err(t("invalid download address").into());
    }
    let url = url::Url::parse(raw).map_err(|_| t("invalid download address"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(t("download addresses cannot contain credentials").into());
    }
    let permitted = match (policy, url.scheme()) {
        (Policy::Web, "http" | "https") | (Policy::Update { .. }, "https") => url.host_str().is_some(),
        (Policy::Update { local_files: true }, "file") => url.host_str().is_none_or(|h| h == "localhost") && url.to_file_path().is_ok(),
        _ => false,
    };
    if !permitted {
        return Err(t("this address uses an unsupported or insecure protocol").into());
    }
    Ok(url.into())
}

pub fn curl(policy: Policy) -> Command {
    #[cfg(windows)]
    let executable = std::env::var_os("SystemRoot").map(|root| std::path::PathBuf::from(root).join("System32").join("curl.exe")).unwrap_or_else(|| "curl.exe".into());
    #[cfg(not(windows))]
    let executable = "curl";
    let mut command = Command::new(executable);
    // --disable must be first: user curl configuration must not change verification,
    // request destinations or output paths. Globs must not expand a single asset URL.
    command.args(["--disable", "--globoff", "--fail", "--silent", "--show-error", "--location", "--max-redirs", "10"]);
    let (protocols, redirects) = match policy {
        Policy::Web => ("=http,https", "=http,https"),
        Policy::Update { local_files: false } => ("=https", "=https"),
        Policy::Update { local_files: true } => ("=https,file", "=https"),
    };
    command.args(["--proto", protocols, "--proto-redir", redirects, "--user-agent"])
        .arg(concat!("TA-Reborn-Launcher/", env!("CARGO_PKG_VERSION"))).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

/// A request curl could not make, in words of the launcher's own (translated), by curl's exit
/// `code`; what curl itself said (`detail`: English, with the address) goes to the log, `what`
/// naming the request.
pub(crate) fn failure(code: Option<i32>, detail: &str, what: &str) -> String {
    let detail = detail.trim().trim_start_matches("curl: ");
    log::warn!("{what}: curl {}: {detail}", code.map_or_else(|| "stopped".to_string(), |code| format!("exit {code}")));
    match code {
        Some(6) => t("the server could not be found: is the computer online?").to_string(),
        Some(5) => t("the proxy could not be found").to_string(),
        Some(7) => t("the server could not be reached").to_string(),
        Some(28) => t("the connection timed out").to_string(),
        Some(22) => match http_status(detail) {
            Some(status) => tf("the server answered with error {code}", &[("code", &status)]),
            None => t("the server answered with an error").to_string(),
        },
        Some(35 | 51 | 53 | 54 | 58 | 59 | 60 | 64 | 66 | 77 | 80 | 82 | 83 | 90 | 91 | 98) => t("a secure connection to the server could not be made").to_string(),
        Some(18 | 52 | 55 | 56 | 92 | 95) => t("the connection was interrupted").to_string(),
        Some(23) => t("the download could not be written to the disk").to_string(),
        Some(63) => t("The download exceeded its expected size.").to_string(),
        Some(47) => t("the server redirected too many times").to_string(),
        Some(37) => t("the file could not be read").to_string(),
        Some(1 | 3) => t("invalid download address").to_string(),
        Some(code) => tf("the request failed (curl error {code})", &[("code", &code)]),
        None => t("the request failed").to_string(),
    }
}

/// The HTTP status in curl's "The requested URL returned error: 404".
fn http_status(detail: &str) -> Option<u16> {
    let (_, status) = detail.rsplit_once("error: ")?;
    status.trim().split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

/// Bound text responses even when the server omits Content-Length or sends chunks.
pub fn text_output(command: Command) -> Result<Output, String> {
    bounded_output(command, METADATA_LIMIT)
}

/// Apply our own bound while reading, independently of Content-Length and the
/// installed curl version's treatment of chunked responses.
pub(crate) fn bounded_output(mut command: Command, limit: usize) -> Result<Output, String> {
    let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| {
        log::warn!("curl did not start: {e}");
        tf("curl could not be started: {error}", &[("error", &e)])
    })?;
    // Drain both pipes concurrently: a long curl diagnostic must not fill stderr
    // while this thread waits for stdout's EOF. Keep diagnostics bounded as well.
    let errors = child.stderr.take().expect("piped errors");
    let reader = std::thread::spawn(move || diagnostics(errors));
    let mut bytes = Vec::new();
    let read = child.stdout.take().expect("piped output").take(limit as u64 + 1).read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > limit {
        let _ = child.kill();
        let _ = child.wait();
        let _ = reader.join();
        return Err(t("the server's response is too large or could not be read").into());
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let stderr = reader.join().unwrap_or_default();
    Ok(Output { status, stdout: bytes, stderr })
}

/// Preserve useful curl diagnostics while draining the whole pipe to avoid blocking it.
pub(crate) fn diagnostics(mut input: impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = (&mut input).take(16 << 10).read_to_end(&mut bytes);
    let _ = std::io::copy(&mut input, &mut std::io::sink());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_addresses_cannot_invoke_options_files_or_protocol_handlers() {
        for raw in ["--version", "--config=payload.txt", "file:///C:/Windows/System32/calc.exe", "goggalaxy://openGameView/1", "javascript:alert(1)", "https://user:secret@example.org/", "https://example.org/\n--insecure", "C:\\Windows\\System32\\calc.exe"] {
            assert!(address(raw, Policy::Web).is_err(), "{raw}");
            assert!(address(raw, Policy::Update { local_files: false }).is_err(), "{raw}");
        }
        assert_eq!(address("HTTPS://example.org/a b", Policy::Web).unwrap(), "https://example.org/a%20b");
        assert!(address("http://example.org/update", Policy::Update { local_files: false }).is_err());
        assert!(address("file://server/share/setup.exe", Policy::Update { local_files: true }).is_err());
        let file = url::Url::from_file_path(std::env::temp_dir().join("release.toml")).unwrap();
        assert!(address(file.as_str(), Policy::Update { local_files: true }).is_ok());
    }

    #[test]
    fn metadata_output_drains_large_diagnostics_without_blocking() {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("powershell.exe");
            command.args(["-NoProfile", "-NonInteractive", "-Command", "[Console]::Error.Write(('x' * 131072)); [Console]::Out.Write('metadata')"]);
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "head -c 131072 /dev/zero >&2; printf metadata"]);
            command
        };
        command.stdin(Stdio::null());
        let output = text_output(command).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"metadata");
        assert_eq!(output.stderr.len(), 16 << 10);
    }

    #[test]
    fn unknown_length_output_is_stopped_at_the_callers_limit() {
        let body = |count| {
            #[cfg(windows)]
            let mut command = {
                let mut command = Command::new("powershell.exe");
                command.args(["-NoProfile", "-NonInteractive", "-Command", &format!("[Console]::Out.Write(('x' * {count}))")]);
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
                command
            };
            #[cfg(not(windows))]
            let mut command = {
                let mut command = Command::new("sh");
                command.args(["-c", &format!("head -c {count} /dev/zero")]);
                command
            };
            command.stdin(Stdio::null());
            command
        };
        // The producer has no headers and no size enforcement of its own, just
        // as an older curl streaming an unknown-length image would behave.
        assert_eq!(bounded_output(body(4096), 4096).unwrap().stdout.len(), 4096);
        assert!(bounded_output(body(8192), 4096).unwrap_err().contains("too large"));
    }

    /// curl's failures are said in words of the launcher's own (translated), the address and
    /// curl's English left to the log.
    #[test]
    fn curl_failures_are_said_in_words_of_our_own() {
        assert_eq!(failure(Some(6), "curl: (6) Could not resolve host: www.moddb.com", "test"), t("the server could not be found: is the computer online?"));
        assert_eq!(failure(Some(22), "curl: (22) The requested URL returned error: 503", "test"), tf("the server answered with error {code}", &[("code", &503)]));
        assert_eq!(failure(Some(22), "", "test"), t("the server answered with an error"));
        assert_eq!(failure(Some(60), "curl: (60) SSL certificate problem", "test"), t("a secure connection to the server could not be made"));
        assert_eq!(failure(Some(99), "curl: (99) something new", "test"), tf("the request failed (curl error {code})", &[("code", &99)]));
        for code in [Some(6), Some(7), Some(22), Some(28), Some(56), Some(99), None] {
            let words = failure(code, "curl: (99) Could not resolve host: example.org", "test");
            assert!(!words.contains("example.org") && !words.contains("curl:"), "{words}");
        }
    }

    #[test]
    fn secure_downloads_cannot_downgrade_or_load_curl_configuration() {
        let command = curl(Policy::Update { local_files: false });
        let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy()).collect();
        assert_eq!(args[0], "--disable");
        assert!(args.windows(2).any(|a| a == ["--proto", "=https"]));
        assert!(args.windows(2).any(|a| a == ["--proto-redir", "=https"]));
        assert!(args.iter().any(|a| a == "--globoff"));
    }
}
