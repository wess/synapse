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

#[test]
fn catalog_and_manifest_work_are_bounded() {
    let root = tempfile::tempdir().unwrap();
    let manifest = include_str!("../../../../plugins/workshop/plugin.toml");
    for number in 0..=PLUGINLIMIT {
        let path = root.path().join(format!("plugin{number}"));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(
            path.join("plugin.toml"),
            manifest.replace("id = \"workshop\"", &format!("id = \"plugin{number}\"")),
        )
        .unwrap();
    }
    let result = catalog(root.path());
    assert_eq!(result.plugins.len(), PLUGINLIMIT + 1);
    assert!(
        result
            .problems
            .iter()
            .any(|problem| problem.contains("128"))
    );

    let large = root.path().join("large");
    std::fs::create_dir(&large).unwrap();
    std::fs::write(
        large.join("plugin.toml"),
        vec![b' '; MANIFESTLIMIT as usize + 1],
    )
    .unwrap();
    assert!(load(&large).unwrap_err().to_string().contains("64 KiB"));
}

#[cfg(unix)]
#[test]
fn rejects_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape.png")).unwrap();
    assert!(asset(root.path(), "escape.png").is_err());
}
