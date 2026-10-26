//! Synthetic guide-index measurements in a disposable repository.
use anyhow::Result;
use serde_json::json;
use std::time::Instant;
use synapsecore::knowledge::{self, WriteRequest};

fn measure(root: &std::path::Path, iterations: usize) -> Result<serde_json::Value> {
    let mut times = Vec::new();
    let mut sourcebytes = 0;
    let mut responsebytes = 0;
    for _ in 0..iterations {
        let start = Instant::now();
        let index = knowledge::catalog(root)?;
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        sourcebytes = index.sourcebytes;
        responsebytes = serde_json::to_vec(&index)?.len();
    }
    times.sort_by(f64::total_cmp);
    Ok(
        json!({"p50_ms":times[iterations / 2], "p95_ms":times[iterations * 95 / 100],
        "source_bytes_per_call":sourcebytes, "response_bytes":responsebytes}),
    )
}

fn main() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    std::fs::create_dir(root.join(".git"))?;
    let empty = measure(root, 40)?;
    let content = vec![b'x'; 64 * 1024];
    let mut sources = Vec::new();
    for number in 0..64 {
        let name = format!("source{number}.rs");
        std::fs::write(root.join(&name), &content)?;
        sources.push(name);
    }
    for number in 0..32 {
        knowledge::write(
            root,
            &WriteRequest {
                project: None,
                name: format!("guide{number}"),
                description: "Synthetic guide".into(),
                content: "# Example\n".into(),
                sources: sources.clone(),
                revision: None,
            },
        )?;
    }
    let populated = measure(root, 40)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"empty":empty, "guides":32,
        "shared_sources":64, "populated":populated}))?
    );
    Ok(())
}
