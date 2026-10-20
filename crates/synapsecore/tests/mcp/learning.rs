use super::*;

fn refused(home: &std::path::Path, data: &std::path::Path, args: &[&str]) {
    let out = Command::new(env!("CARGO_BIN_EXE_synapse-cli"))
        .args(args)
        .env("SYNAPSE_HOME", home)
        .env("SYNAPSE_DATA", data)
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "an unapproved or stale change was accepted"
    );
}

fn proposal(
    client: &mut Session,
    id: i64,
    project: &std::path::Path,
    name: &str,
    body: &str,
) -> String {
    tool(
        client,
        id,
        "revise",
        json!({
            "name":name,"instructions":body,"project":project,
            "note":"The procedure needs the reviewed verification step."
        }),
    )["result"]["structuredContent"]["proposal"]
        .as_i64()
        .unwrap()
        .to_string()
}

#[test]
fn skill_revisions_reject_stale_bases_and_preserve_user_edited_copies() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let data = directory.path().join("data");
    let project = directory.path().join("app");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    run(&home, &data, &["settings", "learn", "on"]);
    run(
        &home,
        &data,
        &[
            "skill",
            "create",
            "releaseflow",
            "--project",
            project.to_str().unwrap(),
        ],
    );
    let shelf = synapsecore::skill::Shelf::project(&project);
    let entry = data
        .join("skills")
        .join(format!("@{}", shelf.key()))
        .join("releaseflow/SKILL.md");
    let original = synapsecore::skill::compose(
        "releaseflow",
        "Use for release verification.",
        "Build the release and verify the package.",
    )
    .unwrap();
    std::fs::write(&entry, &original).unwrap();
    run(
        &home,
        &data,
        &[
            "skill",
            "approve",
            "releaseflow",
            "--project",
            project.to_str().unwrap(),
        ],
    );
    let statuses: Value =
        serde_json::from_str(&run(&home, &data, &["skill", "status", "--json"])).unwrap();
    let copies: Vec<_> = statuses["skills"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["skill"] == "releaseflow" && row["state"] == "installed")
        .map(|row| std::path::PathBuf::from(row["path"].as_str().unwrap()).join("SKILL.md"))
        .collect();
    assert!(copies.len() >= 2);
    let custom = format!("{original}\nA user-added verification step must survive.\n");
    std::fs::write(&copies[0], &custom).unwrap();
    let mut client = session(&home, &data);
    let stale = proposal(
        &mut client,
        10,
        &project,
        "releaseflow",
        "Build, inspect, and verify the release.",
    );
    assert_eq!(std::fs::read_to_string(&entry).unwrap(), original);
    let intervening = synapsecore::skill::compose(
        "releaseflow",
        "Use for release verification.",
        "A user changed this procedure while the suggestion waited.",
    )
    .unwrap();
    std::fs::write(&entry, &intervening).unwrap();
    refused(
        &home,
        &data,
        &[
            "learn",
            "approve",
            &stale,
            "--project",
            project.to_str().unwrap(),
            "--confirm",
        ],
    );
    assert_eq!(std::fs::read_to_string(&entry).unwrap(), intervening);
    let fresh = proposal(
        &mut client,
        11,
        &project,
        "releaseflow",
        "Build, inspect, and verify the reviewed release.",
    );
    run(
        &home,
        &data,
        &[
            "learn",
            "approve",
            &fresh,
            "--project",
            project.to_str().unwrap(),
            "--confirm",
        ],
    );
    assert!(
        std::fs::read_to_string(&entry)
            .unwrap()
            .contains("verify the reviewed release")
    );
    assert_eq!(std::fs::read_to_string(&copies[0]).unwrap(), custom);
    assert!(
        std::fs::read_to_string(&copies[1])
            .unwrap()
            .contains("verify the reviewed release")
    );
    run(
        &home,
        &data,
        &[
            "skill",
            "revert",
            "releaseflow",
            "--project",
            project.to_str().unwrap(),
        ],
    );
    assert_eq!(std::fs::read_to_string(&entry).unwrap(), intervening);
    assert_eq!(std::fs::read_to_string(&copies[0]).unwrap(), custom);
    client.child.kill().ok();
}

#[test]
fn a_global_skill_change_requires_explicit_global_approval() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let data = directory.path().join("data");
    let project = directory.path().join("app");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    run(&home, &data, &["settings", "learn", "on"]);
    run(&home, &data, &["skill", "create", "sharedflow"]);
    let entry = data.join("skills/sharedflow/SKILL.md");
    let original = std::fs::read_to_string(&entry).unwrap();
    let mut client = session(&home, &data);
    let id = proposal(
        &mut client,
        10,
        &project,
        "sharedflow",
        "Inspect the result before accepting the change.",
    );
    refused(
        &home,
        &data,
        &[
            "learn",
            "approve",
            &id,
            "--project",
            project.to_str().unwrap(),
            "--confirm",
        ],
    );
    assert_eq!(std::fs::read_to_string(&entry).unwrap(), original);
    run(
        &home,
        &data,
        &[
            "learn",
            "approve",
            &id,
            "--project",
            project.to_str().unwrap(),
            "--confirm",
            "--global",
        ],
    );
    assert!(
        std::fs::read_to_string(&entry)
            .unwrap()
            .contains("Inspect the result")
    );
    client.child.kill().ok();
}

#[test]
fn learning_tools_keep_claims_unverified_and_remember_exact_drift_resolutions() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let data = root.path().join("data");
    let project = root.path().join("app");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    run(&home, &data, &["settings", "learn", "on"]);
    let mut client = session(&home, &data);
    let advertised = names(&mut client);
    assert!(advertised.iter().any(|name| name == "feedback"));
    assert!(advertised.iter().any(|name| name == "resolvedrift"));
    let first = tool(
        &mut client,
        10,
        "remember",
        json!({
            "content":"Always squash commits when merging a pull request into main.",
            "scope":"project", "project":project,
        }),
    )["result"]["structuredContent"]["id"]
        .as_i64()
        .unwrap();
    let second = tool(
        &mut client,
        11,
        "remember",
        json!({
            "content":"Never squash commits when merging a pull request into main.",
            "scope":"project", "project":project,
        }),
    );
    let finding = second["result"]["structuredContent"]["drift"][0].clone();
    assert_eq!(finding["concern"], "conflict");
    let feedback = json!({"project":project,"feedback":{
        "target":{"kind":"memory","id":first}, "outcome":"unresolved",
        "problem":"Merge policies depend on which branch is being released.",
        "correction":"Check the target branch policy before selecting the merge mode.",
        "evidence":"A retry succeeded; the original cause is still unknown. PASSWORD=hunter2"
    }});
    let receipt =
        tool(&mut client, 12, "feedback", feedback.clone())["result"]["structuredContent"].clone();
    assert_eq!(receipt["verification"], "reported");
    let retry =
        tool(&mut client, 13, "feedback", feedback.clone())["result"]["structuredContent"].clone();
    assert_eq!(receipt["id"], retry["id"]);
    assert!(!receipt.to_string().contains("hunter2"));
    let ledger: Value = serde_json::from_str(&run(
        &home,
        &data,
        &["learn", "list", "--project", project.to_str().unwrap()],
    ))
    .unwrap();
    assert_eq!(ledger.as_array().unwrap().len(), 1);
    assert!(!ledger.to_string().contains("hunter2"));
    let mut forged = feedback;
    forged["feedback"]["verification"] = json!("confirmed");
    let refused = call(
        &mut client.stdin,
        &mut client.stdout,
        json!({
            "jsonrpc":"2.0","id":14,"method":"tools/call",
            "params":{"name":"feedback","arguments":forged}
        }),
        14,
    );
    assert!(refused["error"].is_object() || refused["result"]["isError"] == true);
    assert!(!refused.to_string().contains("hunter2"));
    let resolution = tool(
        &mut client,
        15,
        "resolvedrift",
        json!({
            "project":project,"finding":finding,"disposition":"contextual",
            "note":"The policies refer to different release branches."
        }),
    )["result"]["structuredContent"]["id"]
        .as_i64()
        .unwrap()
        .to_string();
    let recalled = tool(
        &mut client,
        16,
        "recall",
        json!({
            "query":"merge pull request", "project":project,
        }),
    );
    assert!(
        recalled["result"]["structuredContent"]["drift"]
            .as_array()
            .is_none_or(Vec::is_empty)
    );
    run(
        &home,
        &data,
        &[
            "learn",
            "restore",
            &resolution,
            "--project",
            project.to_str().unwrap(),
            "--confirm",
        ],
    );
    let recalled = tool(
        &mut client,
        17,
        "recall",
        json!({
            "query":"merge pull request", "project":project,
        }),
    );
    assert!(
        !recalled["result"]["structuredContent"]["drift"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    client.child.kill().ok();
}

#[test]
fn private_skill_content_is_redacted_for_review_and_never_retained_in_sqlite() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let data = directory.path().join("data");
    let project = directory.path().join("app");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    run(&home, &data, &["settings", "learn", "on"]);
    run(&home, &data, &["skill", "create", "privateflow"]);
    let entry = data.join("skills/privateflow/SKILL.md");
    let original = synapsecore::skill::compose(
        "privateflow",
        "Use for testing.",
        "Check the release.\nPASSWORD=hunter2",
    )
    .unwrap();
    std::fs::write(&entry, &original).unwrap();
    let mut client = session(&home, &data);
    let id = proposal(
        &mut client,
        10,
        &project,
        "privateflow",
        "Inspect the release package.",
    );
    let review = run(
        &home,
        &data,
        &[
            "learn",
            "proposal",
            &id,
            "--project",
            project.to_str().unwrap(),
        ],
    );
    assert!(!review.contains("hunter2"));
    let review: Value = serde_json::from_str(&review).unwrap();
    assert!(review["redacted"].as_u64().unwrap() > 0);
    refused(
        &home,
        &data,
        &[
            "learn",
            "approve",
            &id,
            "--project",
            project.to_str().unwrap(),
            "--confirm",
            "--global",
        ],
    );
    assert_eq!(std::fs::read_to_string(&entry).unwrap(), original);
    client.child.kill().ok();
    client.child.wait().ok();
    for name in ["brain.db", "brain.db-wal"] {
        if let Ok(bytes) = std::fs::read(data.join(name)) {
            assert!(!bytes.windows(7).any(|word| word == b"hunter2"));
        }
    }
}
