use synapsecore::brain::{Brain, EvidenceStatus, MemoryScope};

fn repository(root: &std::path::Path) {
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(
        root.join(".git/config"),
        "[remote \"origin\"]\n\turl = git@github.com:acme/evidence.git\n",
    )
    .unwrap();
}

fn source(root: &std::path::Path, body: &str) {
    let path = root.join("src/session.rs");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

#[tokio::test]
async fn repository_evidence_marks_changed_and_missing_sources_without_hiding_memory() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    repository(&root);
    source(&root, "pub fn refresh() {}\n");
    let brain = Brain::open(directory.path().join("brain.db"))
        .await
        .unwrap();
    let sources = vec!["src/session.rs".to_owned()];
    let (id, _) = brain
        .rememberreplacingevidenced(
            "Refresh rotates the session token.",
            Some("source inspection"),
            MemoryScope::Project,
            Some(&root),
            &[],
            &sources,
        )
        .await
        .unwrap();
    let memory = brain.memory(id).await.unwrap().unwrap();

    let current = brain
        .evidence(&[memory.clone()], Some(&root))
        .await
        .unwrap();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].status, EvidenceStatus::Current);
    assert_eq!(current[0].sources, sources);

    source(&root, "pub fn refresh() { rotate_twice(); }\n");
    let changed = brain
        .evidence(&[memory.clone()], Some(&root))
        .await
        .unwrap();
    assert_eq!(changed[0].status, EvidenceStatus::Needsverification);
    assert_eq!(changed[0].changed, vec!["src/session.rs"]);
    let drift = brain.drift(Some(&root)).await.unwrap();
    assert_eq!(drift.evidence, changed);
    assert!(
        brain
            .recallscoped("rotates session", 4, None, Some(&root))
            .await
            .unwrap()
            .1
            .iter()
            .any(|memory| memory.id == id)
    );

    std::fs::remove_file(root.join("src/session.rs")).unwrap();
    let missing = brain.evidence(&[memory], Some(&root)).await.unwrap();
    assert_eq!(missing[0].status, EvidenceStatus::Needsverification);
    assert_eq!(missing[0].missing, vec!["src/session.rs"]);
}

#[tokio::test]
async fn revalidation_supersedes_the_old_claim_and_an_edit_drops_old_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    repository(&root);
    source(&root, "pub fn refresh() {}\n");
    let brain = Brain::open(directory.path().join("brain.db"))
        .await
        .unwrap();
    let sources = vec!["src/session.rs".to_owned()];
    let (old, _) = brain
        .rememberreplacingevidenced(
            "Refresh rotates one token.",
            None,
            MemoryScope::Project,
            Some(&root),
            &[],
            &sources,
        )
        .await
        .unwrap();
    source(&root, "pub fn refresh() { rotate_twice(); }\n");
    let (new, replaced) = brain
        .rememberreplacingevidenced(
            "Refresh rotates both tokens.",
            None,
            MemoryScope::Project,
            Some(&root),
            &[old],
            &sources,
        )
        .await
        .unwrap();
    assert_eq!(replaced, vec![old]);
    assert_eq!(brain.memory(old).await.unwrap().unwrap().superseded, new);
    let memory = brain.memory(new).await.unwrap().unwrap();
    assert_eq!(
        brain.evidence(&[memory], Some(&root)).await.unwrap()[0].status,
        EvidenceStatus::Current
    );

    brain
        .updatememory(new, "Refresh rotates both active tokens.", None)
        .await
        .unwrap();
    let memory = brain.memory(new).await.unwrap().unwrap();
    assert!(
        brain
            .evidence(&[memory], Some(&root))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn evidence_is_project_only_and_uses_the_knowledge_source_guards() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    repository(&root);
    source(&root, "pub fn refresh() {}\n");
    let brain = Brain::open(directory.path().join("brain.db"))
        .await
        .unwrap();

    let global = brain
        .rememberreplacingevidenced(
            "Global fact",
            None,
            MemoryScope::Global,
            None,
            &[],
            &["src/session.rs".to_owned()],
        )
        .await
        .unwrap_err();
    assert!(global.to_string().contains("project memory"));

    let guarded = brain
        .rememberreplacingevidenced(
            "Credential-derived fact",
            None,
            MemoryScope::Project,
            Some(&root),
            &[],
            &[".env".to_owned()],
        )
        .await
        .unwrap_err();
    assert!(guarded.to_string().contains("credential files"));
}
