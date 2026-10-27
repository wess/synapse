use super::*;
use synapsecore::{brain::Brain, presence};

fn sessions_with_count(
    runtime: &tokio::runtime::Runtime,
    brain: &Brain,
    expected: usize,
) -> Vec<presence::Session> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let rows = runtime.block_on(presence::sessions(brain.pool())).unwrap();
        if rows.len() == expected || std::time::Instant::now() >= deadline {
            return rows;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn transports_publish_presence_without_joining_the_mesh_and_depart_cleanly() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let home = root.path().join("home");
    let mut first = session(&home, &data);
    assert!(!names(&mut first).iter().any(|name| name == "register"));
    let mut second = session(&home, &data);
    names(&mut second);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let brain = runtime
        .block_on(Brain::open(data.join("brain.db")))
        .unwrap();
    // The initialized notification has no response, so the transport may still
    // be recording it when the following tools/list call completes.
    let rows = sessions_with_count(&runtime, &brain, 2);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.online(presence::now())));
    assert!(
        rows.iter()
            .all(|row| row.state(presence::now()) == presence::State::Connected)
    );
    assert_ne!(rows[0].slot, rows[1].slot);
    first.finish();
    let rows = runtime.block_on(presence::sessions(brain.pool())).unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row.online(presence::now()))
            .count(),
        1
    );
    second.finish();
    let rows = runtime.block_on(presence::sessions(brain.pool())).unwrap();
    assert!(rows.iter().all(|row| row.ended > 0));
}

#[test]
fn reported_work_state_reaches_the_scene_session() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let home = root.path().join("home");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let brain = runtime
        .block_on(Brain::open(data.join("brain.db")))
        .unwrap();
    runtime.block_on(brain.setmesh(true)).unwrap();
    let mut worker = session(&home, &data);
    let registered = tool(
        &mut worker,
        2,
        "register",
        json!({"name":"builder","project":root.path()}),
    );
    assert_ne!(registered["result"]["isError"], true);
    tool(
        &mut worker,
        3,
        "reportstatus",
        json!({"status":"working","note":"Crafting the parser"}),
    );
    tool(
        &mut worker,
        4,
        "recall",
        json!({"query": "test", "limit": 1}),
    );
    let rows = runtime.block_on(presence::sessions(brain.pool())).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "builder");
    assert_eq!(rows[0].project, root.path().display().to_string());
    assert_eq!(rows[0].note, "Crafting the parser");
    assert_eq!(rows[0].state(presence::now()), presence::State::Working);
    worker.finish();
}
