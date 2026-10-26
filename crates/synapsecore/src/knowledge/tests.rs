use super::*;
use std::fs;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname = 'app'\n").unwrap();
    dir
}

fn request() -> WriteRequest {
    WriteRequest {
        project: None,
        name: "overview".into(),
        description: "Application architecture".into(),
        content: "# App\n\nA Rust application. See Cargo.toml.\n".into(),
        sources: vec!["Cargo.toml".into()],
        revision: None,
    }
}

#[test]
fn a_guide_roundtrips_and_detects_changed_and_missing_sources() {
    let dir = project();
    assert!(catalog(dir.path()).unwrap().guides.is_empty());
    assert!(!dir.path().join(".synapse").exists());
    let stored = write(dir.path(), &request()).unwrap();
    assert_eq!(stored.status, "current");
    let page = read(dir.path(), "overview", 0).unwrap();
    assert_eq!(page.content, request().content);
    assert_eq!(page.guide.revision, stored.revision);
    assert_eq!(page.sources, vec!["Cargo.toml"]);
    let index = catalog(dir.path()).unwrap();
    assert!(index.overview.unwrap().contains("Rust application"));
    fs::write(dir.path().join("Cargo.toml"), "changed").unwrap();
    let stale = catalog(dir.path()).unwrap();
    assert_eq!(stale.guides[0].status, "needsreview");
    assert_eq!(stale.guides[0].changed, vec!["Cargo.toml"]);
    fs::remove_file(dir.path().join("Cargo.toml")).unwrap();
    assert_eq!(
        read(dir.path(), "overview", 0).unwrap().guide.status,
        "needsreview"
    );
}

#[test]
fn refreshing_requires_the_current_revision_including_manual_edits() {
    let dir = project();
    let mut req = request();
    let stored = write(dir.path(), &req).unwrap();
    assert!(write(dir.path(), &req).is_err());
    req.revision = Some(stored.revision);
    let path = dir.path().join(".synapse/knowledge/overview.md");
    let manual = format!(
        "{}\nHuman correction.\n",
        fs::read_to_string(&path).unwrap()
    );
    fs::write(&path, &manual).unwrap();
    assert!(write(dir.path(), &req).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), manual);
    let page = read(dir.path(), "overview", 0).unwrap();
    req.revision = Some(page.guide.revision);
    req.content = format!("{}\nMore detail.", page.content);
    write(dir.path(), &req).unwrap();
    assert!(
        read(dir.path(), "overview", 0)
            .unwrap()
            .content
            .contains("Human correction")
    );
}

#[test]
fn pages_are_bounded_and_preserve_utf8_offsets() {
    let dir = project();
    let mut req = request();
    req.content = "🦀".repeat(4000);
    write(dir.path(), &req).unwrap();
    let mut assembled = String::new();
    let mut offset = 0;
    loop {
        let page = read(dir.path(), "overview", offset).unwrap();
        assert!(page.content.len() <= 6000);
        assembled.push_str(&page.content);
        match page.next {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert_eq!(assembled, req.content);
    assert!(read(dir.path(), "overview", 1).is_err());
    assert!(read(dir.path(), "overview", 100000).is_err());
    assert!(catalog(dir.path()).unwrap().overview.unwrap().len() <= 1800);
}

#[test]
fn paths_and_credentials_are_rejected_without_writing_a_guide() {
    let dir = project();
    let mut req = request();
    for name in ["../escape", "Overview", "a/b", "", "a_b"] {
        req.name = name.into();
        assert!(write(dir.path(), &req).is_err());
    }
    req = request();
    for source in [
        "../Cargo.toml",
        "/etc/passwd",
        ".env",
        ".git/config",
        "C:\\secrets",
        ".synapse/knowledge/overview.md",
    ] {
        req.sources = vec![source.into()];
        assert!(write(dir.path(), &req).is_err());
    }
    req = request();
    req.content = "<private>do not store this</private>".into();
    assert!(write(dir.path(), &req).is_err());
    assert!(!dir.path().join(".synapse").exists());
}

#[test]
fn guides_are_scoped_to_the_actual_checkout() {
    let one = project();
    let two = project();
    write(one.path(), &request()).unwrap();
    assert!(catalog(two.path()).unwrap().guides.is_empty());
    assert!(read(two.path(), "overview", 0).is_err());
    fs::create_dir(one.path().join("src")).unwrap();
    assert_eq!(catalog(&one.path().join("src")).unwrap().guides.len(), 1);
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_redirect_guides_or_sources_outside_the_project() {
    use std::os::unix::fs::symlink;
    let dir = project();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret"), "secret").unwrap();
    symlink(outside.path(), dir.path().join(".synapse")).unwrap();
    assert!(write(dir.path(), &request()).is_err());
    assert!(catalog(dir.path()).is_err());
    fs::remove_file(dir.path().join(".synapse")).unwrap();
    symlink(outside.path(), dir.path().join("external")).unwrap();
    let mut req = request();
    req.sources = vec!["external/secret".into()];
    assert!(write(dir.path(), &req).is_err());
    write(dir.path(), &request()).unwrap();
    let path = dir.path().join(".synapse/knowledge/database.md");
    symlink(outside.path().join("secret"), &path).unwrap();
    req = request();
    req.name = "database".into();
    assert!(write(dir.path(), &req).is_err());
    assert_eq!(
        fs::read_to_string(outside.path().join("secret")).unwrap(),
        "secret"
    );
}

#[test]
fn malformed_guides_do_not_hide_valid_ones() {
    let dir = project();
    write(dir.path(), &request()).unwrap();
    fs::write(
        dir.path().join(".synapse/knowledge/broken.md"),
        "no metadata",
    )
    .unwrap();
    let index = catalog(dir.path()).unwrap();
    assert_eq!(index.guides.len(), 1);
    assert_eq!(index.warnings.len(), 1);
    assert!(index.warnings[0].contains("broken"));
}

#[tokio::test]
async fn setting_is_off_by_default_persisted_and_shared_by_connections() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("brain.db");
    let brain = crate::brain::Brain::open(&db).await.unwrap();
    assert!(!brain.knowledge().await.unwrap());
    brain.setknowledge(true).await.unwrap();
    let second = crate::brain::Brain::open(&db).await.unwrap();
    assert!(second.knowledge().await.unwrap());
    brain.setknowledge(false).await.unwrap();
    assert!(!second.knowledge().await.unwrap());
}

#[test]
fn setting_gates_both_tools_and_guidance() {
    let names = |enabled| {
        crate::mcp::toolcost(false, false, enabled)
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    };
    assert!(!names(false).iter().any(|name| name.contains("knowledge")));
    assert!(names(true).contains(&"knowledge".to_owned()));
    assert!(names(true).contains(&"writeknowledge".to_owned()));
    let on = crate::instructions::modelfacing("", false, false, true);
    let off = crate::instructions::modelfacing(&on, false, false, false);
    assert!(on.contains("## Project knowledge"));
    assert!(!off.contains("## Project knowledge"));
    assert_eq!(
        crate::instructions::modelfacing(&on, false, false, true),
        on
    );
}

#[test]
fn guides_remain_readable_after_editing_with_windows_line_endings() {
    let dir = project();
    write(dir.path(), &request()).unwrap();
    let path = dir.path().join(".synapse/knowledge/overview.md");
    let text = fs::read_to_string(&path).unwrap().replace("\n", "\r\n");
    fs::write(&path, text).unwrap();
    assert_eq!(
        read(dir.path(), "overview", 0).unwrap().content,
        request().content.replace("\n", "\r\n")
    );
    assert_eq!(catalog(dir.path()).unwrap().guides.len(), 1);
}

#[test]
fn manual_private_content_is_redacted_before_pagination() {
    let dir = project();
    write(dir.path(), &request()).unwrap();
    let path = dir.path().join(".synapse/knowledge/overview.md");
    let text = fs::read_to_string(&path).unwrap();
    let secret = format!("<private>{}</private>", "secret".repeat(2000));
    fs::write(&path, format!("{text}{secret}")).unwrap();
    let page = read(dir.path(), "overview", 0).unwrap();
    assert!(!page.content.contains("secret"));
    assert!(page.next.is_none());
    assert_eq!(page.total, page.content.len());
}
