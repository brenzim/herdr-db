//! Launching the Client.
//!
//! The Client is the third-party terminal database browser the plugin launches and does
//! not reimplement (ADR-0001). Handing it a DSN is the whole of the plugin's contribution
//! at this boundary (ADR-0002).

/// The Client's program name, as it is invoked and as the install-time check looks for it.
pub const PROGRAM: &str = "lazysql";

/// The Client's read-only flag, which it must be given before the positional DSN.
const READ_ONLY: &str = "-read-only";

/// The argv that launches the Client against `dsn`, opened for writing or not.
///
/// The flag goes first. lazysql is a Go program (ADR-0002) and Go's `flag` package stops
/// parsing at the first non-flag argument, so a `-read-only` written after the DSN is
/// accepted without complaint, ignored, and the database opens writable.
pub fn argv(dsn: &str, read_only: bool) -> Vec<String> {
    let mut argv = vec![PROGRAM.to_string()];
    if read_only {
        argv.push(READ_ONLY.to_string());
    }
    argv.push(dsn.to_string());
    argv
}
