//! Guards the root `pyproject.toml` stub that pre-commit installs for the
//! `tree-police` hook: it must pin exactly this crate's version of the
//! PyPI wheel, or hook users get a different binary than the tagged source.

#[test]
fn pre_commit_stub_pins_this_version() {
    let version = env!("CARGO_PKG_VERSION");
    let stub = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/pyproject.toml"))
        .expect("root pyproject.toml should exist");
    assert!(
        stub.contains(&format!("version = \"{version}\"")),
        "root pyproject.toml `version` should be {version}"
    );
    assert!(
        stub.contains(&format!("\"tree-police=={version}\"")),
        "root pyproject.toml should depend on tree-police=={version}"
    );
}
