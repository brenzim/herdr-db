//! The herdr plugin manifest is the plugin's static surface — what herdr reads at link
//! time to know the plugin exists, which Pane it owns, and which action summons it.
//!
//! These parse `herdr-plugin.toml` rather than string-matching it, so they assert the
//! declaration and not its formatting.

use toml::Value;

fn manifest() -> Value {
    toml::from_str::<Value>(include_str!("../herdr-plugin.toml"))
        .expect("herdr-plugin.toml is valid TOML")
}

#[test]
fn identifies_itself_as_the_db_plugin_for_the_platforms_it_supports() {
    let m = manifest();
    assert_eq!(m["id"].as_str(), Some("db"));
    assert_eq!(m["min_herdr_version"].as_str(), Some("0.8.0"));
    assert_eq!(m["platforms"], Value::from(vec!["linux", "macos"]));
}

#[test]
fn declares_one_pane_that_splits_and_runs_the_plugin_binary() {
    let m = manifest();
    let panes = m["panes"].as_array().expect("a [[panes]] entry");
    assert_eq!(panes.len(), 1, "exactly one Pane hosts the Client");

    let pane = &panes[0];
    assert_eq!(pane["placement"].as_str(), Some("split"));
    assert_eq!(
        pane["command"],
        Value::from(vec!["./target/release/herdr-db"]),
        "the Pane body is the plugin binary itself, which execs the Client in place",
    );
}

#[test]
fn declares_one_action_named_open_db_that_runs_the_launcher() {
    let m = manifest();
    let actions = m["actions"].as_array().expect("an [[actions]] entry");
    assert_eq!(
        actions.len(),
        1,
        "browsing is the only action this ticket ships"
    );

    let action = &actions[0];
    assert_eq!(action["id"].as_str(), Some("open-db"));
    assert_eq!(
        action["command"],
        Value::from(vec!["bash", "scripts/open-db.sh"]),
        "herdr actions run a command, so opening the Pane goes through the launcher",
    );
}

#[test]
fn declares_a_build_step_herdr_runs_at_install_time() {
    let m = manifest();
    let builds = m["build"].as_array().expect("a [[build]] entry");
    assert_eq!(
        builds.len(),
        1,
        "one build step, for the two supported platforms"
    );

    assert_eq!(
        builds[0]["command"],
        Value::from(vec!["/bin/sh", "scripts/build.sh"]),
    );
}
