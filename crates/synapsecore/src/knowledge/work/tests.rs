use super::*;

#[tokio::test(flavor = "current_thread")]
async fn blocking_work_is_bounded_and_does_not_stop_the_async_executor() {
    let mut releases = Vec::new();
    let mut workers = Vec::new();
    for _ in 0..2 {
        let (release, parked) = std::sync::mpsc::channel();
        let (ready, started) = tokio::sync::oneshot::channel();
        releases.push(release);
        workers.push(tokio::spawn(run(move || {
            let _ = ready.send(());
            let _ = parked.recv();
            Ok(())
        })));
        tokio::time::timeout(std::time::Duration::from_secs(2), started)
            .await
            .unwrap()
            .unwrap();
    }
    assert!(
        run(|| Ok(()))
            .await
            .unwrap_err()
            .to_string()
            .contains("busy")
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        tokio::time::sleep(std::time::Duration::from_millis(1)),
    )
    .await
    .unwrap();
    for release in releases {
        release.send(()).unwrap();
    }
    for worker in workers {
        worker.await.unwrap().unwrap();
    }
}
