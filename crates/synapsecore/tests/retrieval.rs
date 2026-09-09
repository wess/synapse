use synapsecore::brain::{Brain, MemoryScope, Optimization};

#[test]
fn reading_guidance_reaches_existing_sessions_without_rewriting_their_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("SOUL.md");
    let original = "# My guidance\n\nKeep this wording.\n";
    std::fs::write(&path, original).unwrap();
    let stored = synapsecore::instructions::ensure(&path).unwrap();
    let merged = synapsecore::instructions::modelfacing(&stored, false, false);
    assert!(merged.starts_with(original));
    assert!(merged.contains("`readmemory`"));
    assert_eq!(
        synapsecore::instructions::modelfacing(&merged, false, false),
        merged
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[tokio::test]
async fn read_pages_obey_every_budget_ceiling_even_in_full_mode() {
    let root = tempfile::tempdir().unwrap();
    let brain = Brain::open(root.path().join("brain.db")).await.unwrap();
    let id = brain
        .rememberscoped(&"evidence ".repeat(2_000), None, MemoryScope::Global, None)
        .await
        .unwrap();
    for configured in [
        Optimization::Lean,
        Optimization::Balanced,
        Optimization::Full,
    ] {
        brain.setoptimization(configured).await.unwrap();
        for requested in [
            None,
            Some(Optimization::Lean),
            Some(Optimization::Balanced),
            Some(Optimization::Full),
        ] {
            let response = brain.readscoped(id, 0, requested, None).await.unwrap();
            let expected = configured.constrained(requested);
            let bytes = if expected == Optimization::Lean {
                2_800
            } else {
                6_000
            };
            assert_eq!(response.optimization, expected);
            let page = response.memory.unwrap();
            assert_eq!(page.body.len(), bytes);
            assert_eq!(page.next, Some(bytes as u32));
            assert!(!page.sourceabridged);
        }
    }
}

#[tokio::test]
async fn lean_recall_still_suppresses_exact_duplicate_bodies() {
    let root = tempfile::tempdir().unwrap();
    let brain = Brain::open(root.path().join("brain.db")).await.unwrap();
    for source in ["first observation", "second observation"] {
        brain
            .rememberscoped("shared evidence", Some(source), MemoryScope::Global, None)
            .await
            .unwrap();
    }
    let (_, hits) = brain
        .recallscoped("shared evidence", 4, Some(Optimization::Lean), None)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(brain.stats().await.unwrap().entries, 2);
}

#[tokio::test]
async fn exact_pages_reassemble_unicode_code_whitespace_and_nuls() {
    let root = tempfile::tempdir().unwrap();
    let brain = Brain::open(root.path().join("brain.db")).await.unwrap();
    let body = format!(
        "Evidence.\n```text\n{}\n```",
        "  東京🦀\t\0 detail\n".repeat(1_000)
    );
    let source = "出处🦀".repeat(1_000);
    let id = brain
        .rememberscoped(&body, Some(&source), MemoryScope::Global, None)
        .await
        .unwrap();
    let (_, hits) = brain
        .recallscoped("Evidence", 4, Some(Optimization::Lean), None)
        .await
        .unwrap();
    assert!(hits[0].abridged);
    let mut offset = 0;
    let mut assembled = String::new();
    loop {
        let response = brain
            .readscoped(id, offset, Some(Optimization::Lean), None)
            .await
            .unwrap();
        assert_eq!(response.optimization, Optimization::Lean);
        let page = response.memory.unwrap();
        assert_eq!(page.id, hits[0].id);
        assert_eq!(page.offset, offset);
        assert_eq!(page.total as usize, body.len());
        assert!(page.body.len() <= 2_800);
        assert!(page.source.len() <= 240);
        assert!(page.sourceabridged);
        assert!(source.starts_with(&page.source));
        assert!(!page.body.is_empty());
        assembled.push_str(&page.body);
        let Some(next) = page.next else { break };
        assert_eq!(next as usize, assembled.len());
        assert!(next > offset);
        offset = next;
    }
    assert_eq!(assembled, body);
    let end = brain
        .readscoped(id, body.len() as u32, None, None)
        .await
        .unwrap()
        .memory
        .unwrap();
    assert_eq!(end.body, "");
    assert_eq!(end.next, None);
    assert!(
        brain
            .readscoped(id, body.len() as u32 + 1, None, None)
            .await
            .is_err()
    );
    let inside = body.find('🦀').unwrap() as u32 + 1;
    assert!(brain.readscoped(id, inside, None, None).await.is_err());
}

#[tokio::test]
async fn exact_reads_enforce_scope_supersession_and_missing_ids() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first");
    let second = root.path().join("second");
    std::fs::create_dir_all(first.join("src")).unwrap();
    std::fs::create_dir(first.join(".git")).unwrap();
    std::fs::create_dir(&second).unwrap();
    let brain = Brain::open(root.path().join("brain.db")).await.unwrap();
    let id = brain
        .rememberscoped("private evidence", None, MemoryScope::Project, Some(&first))
        .await
        .unwrap();
    let nested = first.join("src");
    assert!(
        brain
            .readscoped(id, 0, None, Some(&nested))
            .await
            .unwrap()
            .memory
            .is_some()
    );
    for project in [None, Some(second.as_path())] {
        // an invalid offset must not reveal the length or existence of another project's memory.
        assert!(
            brain
                .readscoped(id, u32::MAX, None, project)
                .await
                .unwrap()
                .memory
                .is_none()
        );
    }
    assert!(
        brain
            .readscoped(i64::MAX, 0, None, Some(&first))
            .await
            .unwrap()
            .memory
            .is_none()
    );
    let replacement = brain
        .rememberscoped("new evidence", None, MemoryScope::Global, None)
        .await
        .unwrap();
    brain.supersede(id, replacement).await.unwrap();
    assert!(
        brain
            .readscoped(id, 0, None, Some(&first))
            .await
            .unwrap()
            .memory
            .is_none()
    );
    assert!(
        brain
            .readscoped(replacement, 0, None, Some(&second))
            .await
            .unwrap()
            .memory
            .is_some()
    );
    brain.unsupersede(id).await.unwrap();
    assert!(
        brain
            .readscoped(id, 0, None, Some(&first))
            .await
            .unwrap()
            .memory
            .is_some()
    );
    assert!(brain.readscoped(0, 0, None, None).await.is_err());
}

#[tokio::test]
async fn recall_preserves_negation_numbers_and_short_identifiers() {
    for (first, second) in [
        (
            "Release builds require signed packages validated against the stable production database schema.",
            "Release builds never require signed packages validated against the stable production database schema.",
        ),
        (
            "The production database connection pool keeps at most 5 connections for each running worker.",
            "The production database connection pool keeps at most 50 connections for each running worker.",
        ),
        (
            "Use the eu production database cluster for shared connection pooling across all running workers.",
            "Use the us production database cluster for shared connection pooling across all running workers.",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let brain = Brain::open(root.path().join("brain.db")).await.unwrap();
        brain
            .rememberscoped(first, None, MemoryScope::Global, None)
            .await
            .unwrap();
        brain
            .rememberscoped(second, None, MemoryScope::Global, None)
            .await
            .unwrap();
        for budget in [
            Optimization::Lean,
            Optimization::Balanced,
            Optimization::Full,
        ] {
            let (_, hits) = brain
                .recallscoped("production database", 4, Some(budget), None)
                .await
                .unwrap();
            assert_eq!(
                hits.len(),
                2,
                "lost distinct evidence with {budget:?}: {hits:?}"
            );
        }
    }
}

#[tokio::test]
async fn unicode_queries_keep_their_terms_and_do_not_fall_back_to_recent() {
    let root = tempfile::tempdir().unwrap();
    let brain = Brain::open(root.path().join("brain.db")).await.unwrap();
    for word in ["数据库", "память", "記憶"] {
        let id = brain
            .rememberscoped(word, None, MemoryScope::Global, None)
            .await
            .unwrap();
        brain
            .rememberscoped("unrelated recent entry", None, MemoryScope::Global, None)
            .await
            .unwrap();
        let explanation = brain.explain(word, 4).await.unwrap();
        assert_eq!(explanation.mode(), "search");
        assert_eq!(explanation.kept, vec![word]);
        let (_, hits) = brain
            .recallscoped(word, 4, Some(Optimization::Lean), None)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, id);
    }
}
