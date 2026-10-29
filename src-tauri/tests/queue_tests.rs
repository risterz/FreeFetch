use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

use freefetch_lib::core::config::AppSettings;
use freefetch_lib::core::types::{QueueItem, QueueStatus};
use freefetch_lib::engine::queue::QueueManager;
use freefetch_lib::engine::{EngineManager, StartDownloadParams};
use freefetch_lib::extractor::ExtractorRegistry;
use freefetch_lib::storage::migrations::run_migrations;
use freefetch_lib::storage::{create_in_memory_database, initialize_database};

/// Simple mock HTTP server for testing queue download jobs
async fn run_mock_server<F>(handler: F) -> (SocketAddr, tokio::sync::oneshot::Sender<()>)
where
    F: Fn(String, usize) -> Option<Vec<u8>> + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handler = Arc::new(handler);
    let req_counter = Arc::new(AtomicUsize::new(0));

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accept_res = listener.accept() => {
                    if let Ok((mut socket, _)) = accept_res {
                        let h = handler.clone();
                        let count = req_counter.clone();
                        tokio::spawn(async move {
                            let mut buf = [0u8; 4096];
                            let n = match socket.read(&mut buf).await {
                                Ok(n) if n > 0 => n,
                                _ => return,
                            };
                            let req_str = String::from_utf8_lossy(&buf[..n]).to_string();
                            let req_idx = count.fetch_add(1, Ordering::SeqCst);

                            if let Some(resp_bytes) = h(req_str, req_idx) {
                                let _ = socket.write_all(&resp_bytes).await;
                                let _ = socket.shutdown().await;
                            }
                        });
                    }
                }
            }
        }
    });

    (addr, shutdown_tx)
}

fn sample_params(url: &str, format_id: &str, title: &str, dest_dir: &str) -> StartDownloadParams {
    StartDownloadParams {
        source_url: url.to_string(),
        service_name: "generic".to_string(),
        title: title.to_string(),
        duration: 120,
        thumbnail_url: None,
        format_id: format_id.to_string(),
        download_url: url.to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "bin".to_string(),
        quality_label: "720p".to_string(),
        destination_dir: Some(dest_dir.to_string()),
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
        http_headers: None,
    }
}

// ---------------------------------------------------------------------------
// 1. State Machine Transitions
// ---------------------------------------------------------------------------
#[test]
fn test_state_machine_transitions() {
    // Valid transitions
    assert!(QueueStatus::Queued.can_transition_to(&QueueStatus::Preparing));
    assert!(QueueStatus::Queued.can_transition_to(&QueueStatus::Cancelled));

    assert!(QueueStatus::Preparing.can_transition_to(&QueueStatus::Downloading));
    assert!(QueueStatus::Preparing.can_transition_to(&QueueStatus::Failed));
    assert!(QueueStatus::Preparing.can_transition_to(&QueueStatus::Cancelled));

    assert!(QueueStatus::Downloading.can_transition_to(&QueueStatus::Paused));
    assert!(QueueStatus::Downloading.can_transition_to(&QueueStatus::Processing));
    assert!(QueueStatus::Downloading.can_transition_to(&QueueStatus::Completed));
    assert!(QueueStatus::Downloading.can_transition_to(&QueueStatus::Failed));
    assert!(QueueStatus::Downloading.can_transition_to(&QueueStatus::Cancelled));
    assert!(QueueStatus::Downloading.can_transition_to(&QueueStatus::Retrying));

    assert!(QueueStatus::Paused.can_transition_to(&QueueStatus::Queued));
    assert!(QueueStatus::Paused.can_transition_to(&QueueStatus::Preparing));
    assert!(QueueStatus::Paused.can_transition_to(&QueueStatus::Downloading));
    assert!(QueueStatus::Paused.can_transition_to(&QueueStatus::Cancelled));

    assert!(QueueStatus::Retrying.can_transition_to(&QueueStatus::Downloading));
    assert!(QueueStatus::Retrying.can_transition_to(&QueueStatus::Failed));
    assert!(QueueStatus::Retrying.can_transition_to(&QueueStatus::Cancelled));

    assert!(QueueStatus::Processing.can_transition_to(&QueueStatus::Completed));
    assert!(QueueStatus::Processing.can_transition_to(&QueueStatus::Failed));
    assert!(QueueStatus::Processing.can_transition_to(&QueueStatus::Cancelled));

    assert!(QueueStatus::Failed.can_transition_to(&QueueStatus::Queued));
    assert!(QueueStatus::Cancelled.can_transition_to(&QueueStatus::Queued));

    // Terminal guards
    assert!(!QueueStatus::Completed.can_transition_to(&QueueStatus::Queued));
    assert!(!QueueStatus::Completed.can_transition_to(&QueueStatus::Downloading));
    assert!(!QueueStatus::Completed.can_transition_to(&QueueStatus::Preparing));
    assert!(!QueueStatus::Completed.can_transition_to(&QueueStatus::Failed));

    // Invalid backwards jumps
    assert!(!QueueStatus::Queued.can_transition_to(&QueueStatus::Completed));
    assert!(!QueueStatus::Queued.can_transition_to(&QueueStatus::Processing));
    assert!(!QueueStatus::Failed.can_transition_to(&QueueStatus::Downloading));
}

// ---------------------------------------------------------------------------
// 2. Duplicate Detection
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_duplicate_detection() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let qm = QueueManager::new(repo, engine, settings);

    let temp_dir = std::env::temp_dir().join(format!("ff_q_dup_{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let dest = temp_dir.to_string_lossy().to_string();

    let p1 = sample_params("https://example.com/video1", "1080p", "Video 1", &dest);
    let res1 = qm.add(p1.clone()).await;
    assert!(res1.is_ok(), "First add should succeed");

    // Exact duplicate (same URL + same format)
    let res_dup = qm.add(p1).await;
    assert!(
        res_dup.is_err(),
        "Identical URL + format should be rejected as duplicate"
    );

    // Same URL + different format -> allowed
    let p_diff_fmt = sample_params("https://example.com/video1", "720p", "Video 1", &dest);
    let res_fmt = qm.add(p_diff_fmt).await;
    assert!(
        res_fmt.is_ok(),
        "Same URL with different format should be allowed"
    );

    // Different URL -> allowed
    let p2 = sample_params("https://example.com/video2", "1080p", "Video 2", &dest);
    let res2 = qm.add(p2).await;
    assert!(res2.is_ok(), "Different URL should be allowed");

    // YouTube canonicalization check
    let yt1 = sample_params(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ&feature=share",
        "1080p",
        "Rick",
        &dest,
    );
    let yt2 = sample_params("https://youtu.be/dQw4w9WgXcQ", "1080p", "Rick", &dest);
    let res_yt1 = qm.add(yt1).await;
    assert!(res_yt1.is_ok());
    let res_yt2 = qm.add(yt2).await;
    assert!(
        res_yt2.is_err(),
        "YouTube shortlink pointing to same canonical ID should be rejected as duplicate"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------------------------------------------------------------------------
// 3. Queue Reordering, Movement, and Deletion
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_queue_reordering_and_movement() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let qm = QueueManager::new(repo, engine, settings);

    let temp_dir = std::env::temp_dir().join(format!("ff_q_reorder_{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let dest = temp_dir.to_string_lossy().to_string();

    let item0 = qm
        .add(sample_params(
            "https://example.com/item0",
            "f0",
            "Item 0",
            &dest,
        ))
        .await
        .unwrap();
    let item1 = qm
        .add(sample_params(
            "https://example.com/item1",
            "f1",
            "Item 1",
            &dest,
        ))
        .await
        .unwrap();
    let item2 = qm
        .add(sample_params(
            "https://example.com/item2",
            "f2",
            "Item 2",
            &dest,
        ))
        .await
        .unwrap();

    let items = qm.get_items().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].id, item0.id);
    assert_eq!(items[0].position, 0);
    assert_eq!(items[1].id, item1.id);
    assert_eq!(items[1].position, 1);
    assert_eq!(items[2].id, item2.id);
    assert_eq!(items[2].position, 2);

    // Move item0 down
    qm.move_down(&item0.id).await.unwrap();
    let items_after_down = qm.get_items().unwrap();
    assert_eq!(items_after_down[0].id, item1.id);
    assert_eq!(items_after_down[0].position, 0);
    assert_eq!(items_after_down[1].id, item0.id);
    assert_eq!(items_after_down[1].position, 1);

    // Move item0 up
    qm.move_up(&item0.id).await.unwrap();
    let items_after_up = qm.get_items().unwrap();
    assert_eq!(items_after_up[0].id, item0.id);
    assert_eq!(items_after_up[1].id, item1.id);

    // Reorder: Move item2 to position 0
    qm.reorder(&item2.id, 0).await.unwrap();
    let items_reordered = qm.get_items().unwrap();
    assert_eq!(items_reordered[0].id, item2.id);
    assert_eq!(items_reordered[0].position, 0);

    // Remove item0
    qm.remove(&item0.id).await.unwrap();
    let items_after_remove = qm.get_items().unwrap();
    assert_eq!(items_after_remove.len(), 2);
    assert!(!items_after_remove.iter().any(|i| i.id == item0.id));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------------------------------------------------------------------------
// 4. Clear Operations (Completed & Failed)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_queue_clear_operations() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let qm = QueueManager::new(repo.clone(), engine, settings);

    let temp_dir = std::env::temp_dir().join(format!("ff_q_clear_{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let dest = temp_dir.to_string_lossy().to_string();

    let item_c = qm
        .add(sample_params("https://example.com/c", "fc", "Comp", &dest))
        .await
        .unwrap();
    let item_f = qm
        .add(sample_params("https://example.com/f", "ff", "Fail", &dest))
        .await
        .unwrap();
    let item_q = qm
        .add(sample_params(
            "https://example.com/q",
            "fq",
            "Queued",
            &dest,
        ))
        .await
        .unwrap();

    // Artificially mark statuses
    repo.update_queue_item_status(
        &item_c.id,
        &QueueStatus::Completed,
        None,
        None,
        None,
        Some(100),
    )
    .unwrap();
    repo.update_queue_item_status(
        &item_f.id,
        &QueueStatus::Failed,
        Some("ERROR"),
        Some("Err"),
        None,
        None,
    )
    .unwrap();

    let summary_before = qm.get_summary().unwrap();
    assert_eq!(summary_before.completed_count, 1);
    assert_eq!(summary_before.failed_count, 1);
    assert_eq!(summary_before.queued_count, 1);

    // Clear completed
    qm.clear_completed().await.unwrap();
    assert!(repo.get_queue_item(&item_c.id).unwrap().is_none());
    assert!(repo.get_queue_item(&item_f.id).unwrap().is_some());
    assert!(repo.get_queue_item(&item_q.id).unwrap().is_some());

    // Clear failed
    qm.clear_failed().await.unwrap();
    assert!(repo.get_queue_item(&item_f.id).unwrap().is_none());
    assert!(repo.get_queue_item(&item_q.id).unwrap().is_some());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------------------------------------------------------------------------
// 5. Concurrency Clamping & Dynamic Updates
// ---------------------------------------------------------------------------
#[test]
fn test_queue_concurrency_clamping() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings {
        max_concurrent_downloads: 2,
        ..Default::default()
    }));
    let qm = QueueManager::new(repo, engine, settings);

    assert_eq!(qm.get_concurrency(), 2);

    // Update within valid 1-5 range
    let _ = qm.set_concurrency(4);
    assert_eq!(qm.get_concurrency(), 4);

    // Clamping: lower bound 1
    let _ = qm.set_concurrency(0);
    assert_eq!(qm.get_concurrency(), 1);

    // Clamping: upper bound 5
    let _ = qm.set_concurrency(10);
    assert_eq!(qm.get_concurrency(), 5);
}

// ---------------------------------------------------------------------------
// 6. SQLite Crash Recovery Resets Interrupted Items
// ---------------------------------------------------------------------------
#[test]
fn test_sqlite_crash_recovery_resets_interrupted_queue_items() {
    let temp_dir = std::env::temp_dir().join(format!("ff_q_recovery_{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let db_path = temp_dir.join("history.db");

    let repo = initialize_database(&db_path).unwrap();

    // Create 4 items simulating pre-crash state
    let item_downloading = QueueItem {
        id: "item-dl".to_string(),
        url: "https://example.com/dl".to_string(),
        canonical_url: "https://example.com/dl".to_string(),
        title: "Item DL".to_string(),
        thumbnail: None,
        status: QueueStatus::Downloading,
        selected_format: "1080p".to_string(),
        format_id: "1080p".to_string(),
        output_path: "item-dl.mp4".to_string(),
        file_size: 1000,
        downloaded_bytes: 450,
        progress: 0.45,
        speed: 100,
        eta: 5,
        error_code: None,
        error_message: None,
        retry_count: 0,
        created_at: 10,
        started_at: Some(10),
        completed_at: None,
        updated_at: 10,
        position: 0,
        params_json: "{}".to_string(),
    };

    let item_processing = QueueItem {
        id: "item-proc".to_string(),
        url: "https://example.com/proc".to_string(),
        canonical_url: "https://example.com/proc".to_string(),
        title: "Item Proc".to_string(),
        thumbnail: None,
        status: QueueStatus::Processing,
        selected_format: "720p".to_string(),
        format_id: "720p".to_string(),
        output_path: "item-proc.mp4".to_string(),
        file_size: 1000,
        downloaded_bytes: 1000,
        progress: 1.0,
        speed: 0,
        eta: 0,
        error_code: None,
        error_message: None,
        retry_count: 0,
        created_at: 10,
        started_at: Some(10),
        completed_at: None,
        updated_at: 10,
        position: 1,
        params_json: "{}".to_string(),
    };

    let item_completed = QueueItem {
        id: "item-comp".to_string(),
        url: "https://example.com/comp".to_string(),
        canonical_url: "https://example.com/comp".to_string(),
        title: "Item Comp".to_string(),
        thumbnail: None,
        status: QueueStatus::Completed,
        selected_format: "480p".to_string(),
        format_id: "480p".to_string(),
        output_path: "item-comp.mp4".to_string(),
        file_size: 500,
        downloaded_bytes: 500,
        progress: 1.0,
        speed: 0,
        eta: 0,
        error_code: None,
        error_message: None,
        retry_count: 0,
        created_at: 10,
        started_at: Some(10),
        completed_at: Some(20),
        updated_at: 20,
        position: 2,
        params_json: "{}".to_string(),
    };

    let item_queued = QueueItem {
        id: "item-queued".to_string(),
        url: "https://example.com/q".to_string(),
        canonical_url: "https://example.com/q".to_string(),
        title: "Item Queued".to_string(),
        thumbnail: None,
        status: QueueStatus::Queued,
        selected_format: "360p".to_string(),
        format_id: "360p".to_string(),
        output_path: "item-queued.mp4".to_string(),
        file_size: 0,
        downloaded_bytes: 0,
        progress: 0.0,
        speed: 0,
        eta: 0,
        error_code: None,
        error_message: None,
        retry_count: 0,
        created_at: 10,
        started_at: None,
        completed_at: None,
        updated_at: 10,
        position: 3,
        params_json: "{}".to_string(),
    };

    repo.insert_queue_item(&item_downloading).unwrap();
    repo.insert_queue_item(&item_processing).unwrap();
    repo.insert_queue_item(&item_completed).unwrap();
    repo.insert_queue_item(&item_queued).unwrap();

    // Simulate crash restart: re-run migrations
    let mut conn = rusqlite::Connection::open(&db_path).unwrap();
    run_migrations(&mut conn).unwrap();

    // Verify recovery behavior
    let rec_dl = repo.get_queue_item("item-dl").unwrap().unwrap();
    assert_eq!(
        rec_dl.status,
        QueueStatus::Paused,
        "Interrupted downloading item should safely reset to Paused"
    );
    assert_eq!(rec_dl.downloaded_bytes, 450);

    let rec_proc = repo.get_queue_item("item-proc").unwrap().unwrap();
    assert_eq!(
        rec_proc.status,
        QueueStatus::Paused,
        "Interrupted processing item should safely reset to Paused"
    );

    let rec_comp = repo.get_queue_item("item-comp").unwrap().unwrap();
    assert_eq!(
        rec_comp.status,
        QueueStatus::Completed,
        "Completed items must remain Completed"
    );

    let rec_q = repo.get_queue_item("item-queued").unwrap().unwrap();
    assert_eq!(
        rec_q.status,
        QueueStatus::Queued,
        "Queued items must remain Queued"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------------------------------------------------------------------------
// 7. Concurrency Control and Failure Isolation
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_queue_concurrency_and_failure_isolation() {
    // Setup mock server serving payload for good URLs, 404 for bad URLs
    let payload = vec![0xAB; 2048];
    let (addr, shutdown_tx) = run_mock_server(move |req, _idx| {
        if req.contains("/fail") {
            let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
            return Some(resp.as_bytes().to_vec());
        }

        // Return complete file with 200 OK
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
            payload.len()
        );
        let mut resp = header.into_bytes();
        resp.extend_from_slice(&payload);
        Some(resp)
    })
    .await;

    let temp_dir = std::env::temp_dir().join(format!("ff_q_iso_{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let dest_dir = temp_dir.to_string_lossy().to_string();

    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings {
        max_concurrent_downloads: 2,
        speed_limit_bytes_per_sec: None,
        ..Default::default()
    }));

    let qm = QueueManager::new(repo.clone(), engine, settings);

    // Job 1: Succeeds
    let url1 = format!("http://{addr}/good1.bin");
    let p1 = sample_params(&url1, "f1", "Good Job 1", &dest_dir);
    let item1 = qm.add(p1).await.unwrap();

    // Job 2: Fails
    let url2 = format!("http://{addr}/fail.bin");
    let p2 = sample_params(&url2, "f2", "Fail Job 2", &dest_dir);
    let item2 = qm.add(p2).await.unwrap();

    // Job 3: Succeeds
    let url3 = format!("http://{addr}/good3.bin");
    let p3 = sample_params(&url3, "f3", "Good Job 3", &dest_dir);
    let item3 = qm.add(p3).await.unwrap();

    // Start queue processing
    qm.start_all().await.unwrap();

    // Wait for jobs to settle (max 10 seconds)
    let start_wait = std::time::Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;

        let cur1 = repo.get_queue_item(&item1.id).unwrap().unwrap();
        let cur2 = repo.get_queue_item(&item2.id).unwrap().unwrap();
        let cur3 = repo.get_queue_item(&item3.id).unwrap().unwrap();

        let all_settled = matches!(cur1.status, QueueStatus::Completed | QueueStatus::Failed)
            && matches!(cur2.status, QueueStatus::Completed | QueueStatus::Failed)
            && matches!(cur3.status, QueueStatus::Completed | QueueStatus::Failed);

        if all_settled || start_wait.elapsed() > Duration::from_secs(10) {
            break;
        }
    }

    let final1 = repo.get_queue_item(&item1.id).unwrap().unwrap();
    let final2 = repo.get_queue_item(&item2.id).unwrap().unwrap();
    let final3 = repo.get_queue_item(&item3.id).unwrap().unwrap();

    assert_eq!(
        final1.status,
        QueueStatus::Completed,
        "Job 1 should have completed successfully"
    );
    assert_eq!(
        final2.status,
        QueueStatus::Failed,
        "Job 2 should have failed due to HTTP 404"
    );
    assert_eq!(
        final3.status,
        QueueStatus::Completed,
        "Job 3 must complete successfully despite Job 2 failing (Failure Isolation)"
    );

    let _ = shutdown_tx.send(());
    let _ = std::fs::remove_dir_all(&temp_dir);
}
