//! Launching the Client.
//!
//! The Client is the third-party terminal database browser the plugin launches and does
//! not reimplement (ADR-0001). Handing it a DSN is the whole of the plugin's contribution
//! at this boundary (ADR-0002).

/// The Client's program name, as it is invoked and as the install-time check looks for it.
pub const PROGRAM: &str = "lazysql";

/// The argv that launches the Client against `dsn`.
pub fn argv(dsn: &str) -> Vec<String> {
    vec![PROGRAM.to_string(), dsn.to_string()]
}
