//! The Client boundary: the plugin's contribution ends at handing lazysql a DSN.
//!
//! ADR-0002 records that lazysql takes the DSN as a positional argument, `lazysql <dsn>`,
//! so the DSN is the entire integration surface. Pinning that here is what keeps the Client
//! swappable — a different Client changes this function and the install-time check in
//! `scripts/build.sh`, and nothing else.

use herdr_db::client;

#[test]
fn launches_the_client_against_the_dsn_and_nothing_else() {
    assert_eq!(
        client::argv("postgres://app:secret@localhost:5433/app", false),
        ["lazysql", "postgres://app:secret@localhost:5433/app"],
    );
}

#[test]
fn the_read_only_flag_precedes_the_positional_dsn() {
    // Position, not presence, is what makes this work: lazysql is a Go program (ADR-0002)
    // and Go's `flag` package stops parsing at the first non-flag argument, so a
    // `-read-only` written after the DSN is accepted, ignored, and opens read-write.
    assert_eq!(
        client::argv("postgres://app:secret@localhost:5433/app", true),
        [
            "lazysql",
            "-read-only",
            "postgres://app:secret@localhost:5433/app",
        ],
    );
}
