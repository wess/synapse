use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Output, Stdio};

struct Cli {
    _directory: tempfile::TempDir,
    root: std::path::PathBuf,
    data: std::path::PathBuf,
    home: std::path::PathBuf,
}

impl Cli {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("app");
        let data = directory.path().join("data");
        let home = directory.path().join("home");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let cli = Self {
            _directory: directory,
            root,
            data,
            home,
        };
        cli.ok(&["settings", "learn", "on"], None);
        cli
    }

    fn run(&self, args: &[&str], input: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_synapse-cli"))
            .args(args)
            .current_dir(&self.root)
            .env("SYNAPSE_DATA", &self.data)
            .env("SYNAPSE_HOME", &self.home)
            .env_remove("SYNAPSE_PROJECT_DIR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(input) = input {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
        }
        child.wait_with_output().unwrap()
    }

    fn ok(&self, args: &[&str], input: Option<&str>) -> String {
        let out = self.run(args, input);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn json(&self, args: &[&str], input: Option<&str>) -> Value {
        serde_json::from_str(&self.ok(args, input)).unwrap()
    }
}

#[test]
fn the_headless_learning_loop_checks_proposes_reviews_and_replays() {
    let cli = Cli::new();
    cli.ok(
        &["memory", "add", "--project", cli.root.to_str().unwrap()],
        Some("Release merge history is retained by preserving reviewed commits."),
    );
    let memories = cli.json(&["memory", "list", "--json"], None);
    let memory = memories[0]["id"].as_i64().unwrap();
    let feedback = json!({
        "target": {"kind":"memory","id":memory}, "outcome":"helpful",
        "problem":"Release automation needs the reviewed history.",
        "correction":"Preserve reviewed commits when merging the release branch.",
        "evidence":"The check retained the reviewed history.",
        "query":"release merge history", "expected":"preserving reviewed commits"
    })
    .to_string();
    for session in ["one", "two", "three"] {
        let record = cli.json(&["learn", "report", "--session", session], Some(&feedback));
        assert_eq!(record["verification"], "reported");
        let id = record["id"].as_i64().unwrap().to_string();
        let checked = cli.json(
            &[
                "learn",
                "check",
                &id,
                "--",
                env!("CARGO_BIN_EXE_synapse-cli"),
                "version",
            ],
            None,
        );
        assert_eq!(checked["feedback"]["verification"], "checkpassed");
    }
    let proposals = cli.json(&["learn", "proposals"], None);
    assert_eq!(proposals.as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read_dir(&cli.root).unwrap().count(), 1);
    let id = proposals[0]["id"].as_i64().unwrap().to_string();
    let review = cli.json(&["learn", "proposal", &id], None);
    assert!(
        review["proposal"]["instructions"]
            .as_str()
            .unwrap()
            .contains("Preserve reviewed commits")
    );
    assert_eq!(review["assessment"]["support"], 3);
    assert_eq!(review["fresh"], true);
    assert!(!cli.run(&["learn", "approve", &id], None).status.success());
    let approved = std::thread::scope(|scope| {
        let first = scope.spawn(|| cli.run(&["learn", "approve", &id, "--confirm"], None));
        let second = scope.spawn(|| cli.run(&["learn", "approve", &id, "--confirm"], None));
        [first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(
        approved.iter().filter(|out| out.status.success()).count(),
        1
    );
    assert!(
        cli.json(&["learn", "proposals"], None)
            .as_array()
            .unwrap()
            .is_empty()
    );
    let cases = cli.json(&["learn", "cases"], None);
    let case = cases[0]["id"].as_i64().unwrap().to_string();
    assert!(!cli.run(&["learn", "eval"], None).status.success());
    cli.ok(&["learn", "review", &case, "--accept", "--confirm"], None);
    let evaluated = cli.json(&["learn", "eval", "--json", "--fail-under", "1"], None);
    assert_eq!(evaluated["found"], 1.0);
    assert!(evaluated["cases"][0]["characters"].as_u64().unwrap() > 0);
}

#[test]
fn invalid_inputs_do_not_leak_contents_or_silently_accept_flags() {
    let cli = Cli::new();
    let invalid = cli.run(
        &["learn", "report", "--session", "one"],
        Some(r#"{"target":"PASSWORD=hunter2"}"#),
    );
    assert!(!invalid.status.success());
    assert!(!String::from_utf8_lossy(&invalid.stderr).contains("hunter2"));
    assert!(
        !cli.run(&["learn", "list", "--timeout", "5"], None)
            .status
            .success()
    );
    assert!(
        !cli.run(&["learn", "eval", "--fail-under", "NaN"], None)
            .status
            .success()
    );
    assert!(!cli.run(&["learn", "confirm", "1"], None).status.success());
}

#[test]
fn resolved_stale_warnings_stay_quiet_in_the_next_session_without_the_gui() {
    let cli = Cli::new();
    let body = "The release merge policy lives in `config/merge.toml` for this project.";
    cli.ok(
        &["memory", "add", "--project", cli.root.to_str().unwrap()],
        Some(body),
    );
    let drift = cli.json(&["memory", "drift", "--json"], None);
    let finding = drift["findings"][0].clone();
    assert_eq!(finding["concern"], "stale");
    let before = cli.json(&["session"], Some(&json!({"cwd":cli.root}).to_string()));
    assert!(
        !before["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains(body)
    );
    let resolution = cli.json(
        &["learn", "resolve", "--confirm"],
        Some(
            &json!({
                "finding":finding,"disposition":"falsepositive",
                "note":"This is a template path used by the generated release configuration."
            })
            .to_string(),
        ),
    );
    let after = cli.json(&["session"], Some(&json!({"cwd":cli.root}).to_string()));
    assert!(
        after["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains(body)
    );
    let id = resolution["id"].as_i64().unwrap().to_string();
    cli.ok(&["learn", "restore", &id, "--confirm"], None);
    assert!(
        !cli.json(&["memory", "drift", "--json"], None)["findings"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
