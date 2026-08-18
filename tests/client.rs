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
        client::argv("postgres://app:secret@localhost:5433/app"),
        ["lazysql", "postgres://app:secret@localhost:5433/app"],
    );
}
