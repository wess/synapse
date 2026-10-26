#![cfg(test)]
use super::*;

#[test]
fn loads_external_scene_and_isolates_invalid_plugins() {
    let root = tempfile::tempdir().unwrap();
    let good = root.path().join("garden");
    std::fs::create_dir(&good).unwrap();
    let text = include_str!("../../../../plugins/workshop/plugin.toml")
        .replace("id = \"workshop\"", "id = \"garden\"");
    std::fs::write(good.join("plugin.toml"), &text).unwrap();
    let bad = root.path().join("bad");
    std::fs::create_dir(&bad).unwrap();
    std::fs::write(
        bad.join("plugin.toml"),
        text.replace("api = 1", "api = 999"),
    )
    .unwrap();
    let result = catalog(root.path());
    assert_eq!(result.plugins.len(), 2);
    assert_eq!(result.problems.len(), 1);
    assert!(result.problems[0].contains("unsupported plugin API"));
}

#[test]
fn rejects_traversal_and_non_png_assets() {
    let root = tempfile::tempdir().unwrap();
    assert!(asset(root.path(), "../other.png").is_err());
    assert!(asset(root.path(), "/tmp/other.png").is_err());
    assert!(asset(root.path(), "code.js").is_err());
    assert!(colorvalue("#ffffff").is_ok());
    assert!(colorvalue("#gggggg").is_err());
}

#[cfg(unix)]
#[test]
fn rejects_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape.png")).unwrap();
    assert!(asset(root.path(), "escape.png").is_err());
}
