use freefetch_lib::core::types::{
    DownloadRecord, DownloadStatistics, DownloadStatus, HistoryFilter,
};
use freefetch_lib::storage::create_in_memory_database;

fn create_sample_record(
    id: &str,
    title: &str,
    container: &str,
    status: DownloadStatus,
    duration: u64,
    downloaded_bytes: u64,
    created_at: i64,
) -> DownloadRecord {
    let completed_at = if matches!(status, DownloadStatus::Completed) {
        Some(created_at + 10)
    } else {
        None
    };

    DownloadRecord {
        id: id.to_string(),
        source_url: format!("https://example.com/watch?v={id}"),
        service_name: "youtube".to_string(),
        title: title.to_string(),
        duration,
        thumbnail_url: Some(format!("https://example.com/thumb_{id}.jpg")),
        destination_path: format!("C:\\Downloads\\{title}.{container}"),
        container: container.to_string(),
        quality_label: if container == "mp3" || container == "flac" {
            "Audio Best".to_string()
        } else {
            "1080p FHD".to_string()
        },
        total_bytes: downloaded_bytes,
        downloaded_bytes,
        status,
        error_message: None,
        created_at,
        completed_at,
    }
}

#[test]
fn test_history_empty_repo() {
    let repo = create_in_memory_database().expect("Should create in-memory SQLite");

    let history = repo
        .get_history(&HistoryFilter::default())
        .expect("get_history should succeed");
    assert_eq!(history.total_count, 0);
    assert!(history.items.is_empty());
    assert_eq!(history.limit, 50);
    assert_eq!(history.offset, 0);

    let stats = repo
        .get_statistics()
        .expect("get_statistics should succeed");
    assert_eq!(stats, DownloadStatistics::default());
}

#[test]
fn test_media_type_classification() {
    let audio_containers = [
        "mp3", "m4a", "flac", "opus", "aac", "ogg", "wav", "MP3", "FLAC",
    ];
    for ext in audio_containers {
        let rec = create_sample_record(
            "1",
            "Audio Song",
            ext,
            DownloadStatus::Completed,
            180,
            5_000_000,
            1700000000,
        );
        assert_eq!(rec.media_type(), "audio", "Expected {ext} to be audio");
    }

    let video_containers = ["mp4", "mkv", "webm", "avi", "mov", "MP4"];
    for ext in video_containers {
        let rec = create_sample_record(
            "2",
            "Video Clip",
            ext,
            DownloadStatus::Completed,
            180,
            50_000_000,
            1700000000,
        );
        assert_eq!(rec.media_type(), "video", "Expected {ext} to be video");
    }
}

#[test]
fn test_history_filtering_and_search() {
    let repo = create_in_memory_database().expect("Should create in-memory SQLite");

    // Insert dataset
    repo.insert_download(&create_sample_record(
        "id1",
        "Rust Tutorial Episode 1",
        "mp4",
        DownloadStatus::Completed,
        600,
        50_000_000,
        1700000001,
    ))
    .unwrap();
    repo.insert_download(&create_sample_record(
        "id2",
        "Rust Podcast Ep 42",
        "mp3",
        DownloadStatus::Completed,
        1800,
        30_000_000,
        1700000002,
    ))
    .unwrap();
    repo.insert_download(&create_sample_record(
        "id3",
        "Cooking Pasta Carbonara",
        "mp4",
        DownloadStatus::Failed,
        300,
        0,
        1700000003,
    ))
    .unwrap();
    repo.insert_download(&create_sample_record(
        "id4",
        "Synthwave Relaxing Beat",
        "flac",
        DownloadStatus::Downloading,
        240,
        10_000_000,
        1700000004,
    ))
    .unwrap();
    repo.insert_download(&create_sample_record(
        "id5",
        "Nature Documentary 4K",
        "webm",
        DownloadStatus::Cancelled,
        1200,
        5_000_000,
        1700000005,
    ))
    .unwrap();

    // 1. Filter by status: completed
    let completed = repo
        .get_history(&HistoryFilter {
            status: Some("completed".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(completed.total_count, 2);
    assert!(completed
        .items
        .iter()
        .all(|i| i.status == DownloadStatus::Completed));

    // 2. Filter by status: active (downloading/preparing/queued/processing/retrying)
    let active = repo
        .get_history(&HistoryFilter {
            status: Some("active".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(active.total_count, 1);
    assert_eq!(active.items[0].id, "id4");

    // 3. Filter by media_type: audio
    let audio = repo
        .get_history(&HistoryFilter {
            media_type: Some("audio".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(audio.total_count, 2);
    assert!(audio.items.iter().all(|i| i.media_type() == "audio"));

    // 4. Filter by media_type: video
    let video = repo
        .get_history(&HistoryFilter {
            media_type: Some("video".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(video.total_count, 3);
    assert!(video.items.iter().all(|i| i.media_type() == "video"));

    // 5. Search: title substring "rust" (case-insensitive)
    let search_rust = repo
        .get_history(&HistoryFilter {
            search: Some("rust".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(search_rust.total_count, 2);

    // 6. Search: URL match
    let search_url = repo
        .get_history(&HistoryFilter {
            search: Some("v=id3".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(search_url.total_count, 1);
    assert_eq!(search_url.items[0].id, "id3");

    // 7. SQL injection attack test in search
    let injection = repo
        .get_history(&HistoryFilter {
            search: Some("' OR 1=1 --".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(injection.total_count, 0);
}

#[test]
fn test_history_pagination_and_ordering() {
    let repo = create_in_memory_database().expect("Should create in-memory SQLite");

    for i in 1..=10 {
        repo.insert_download(&create_sample_record(
            &format!("item_{i}"),
            &format!("Item Title {i}"),
            "mp4",
            DownloadStatus::Completed,
            100,
            1_000_000,
            1700000000 + i as i64,
        ))
        .unwrap();
    }

    // Page 1: limit 4, offset 0 -> item_10, item_9, item_8, item_7
    let p1 = repo
        .get_history(&HistoryFilter {
            limit: Some(4),
            offset: Some(0),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(p1.total_count, 10);
    assert_eq!(p1.items.len(), 4);
    assert_eq!(p1.items[0].id, "item_10");
    assert_eq!(p1.items[1].id, "item_9");
    assert_eq!(p1.items[2].id, "item_8");
    assert_eq!(p1.items[3].id, "item_7");

    // Page 2: limit 4, offset 4 -> item_6, item_5, item_4, item_3
    let p2 = repo
        .get_history(&HistoryFilter {
            limit: Some(4),
            offset: Some(4),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(p2.total_count, 10);
    assert_eq!(p2.items.len(), 4);
    assert_eq!(p2.items[0].id, "item_6");

    // Page 3: limit 4, offset 8 -> item_2, item_1
    let p3 = repo
        .get_history(&HistoryFilter {
            limit: Some(4),
            offset: Some(8),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(p3.total_count, 10);
    assert_eq!(p3.items.len(), 2);
    assert_eq!(p3.items[0].id, "item_2");
    assert_eq!(p3.items[1].id, "item_1");
}

#[test]
fn test_download_statistics_aggregates() {
    let repo = create_in_memory_database().expect("Should create in-memory SQLite");

    // 2 Completed Video (mp4, 50MB, 120s each)
    repo.insert_download(&create_sample_record(
        "v1",
        "Video 1",
        "mp4",
        DownloadStatus::Completed,
        120,
        50_000_000,
        1700000001,
    ))
    .unwrap();
    repo.insert_download(&create_sample_record(
        "v2",
        "Video 2",
        "mp4",
        DownloadStatus::Completed,
        120,
        50_000_000,
        1700000002,
    ))
    .unwrap();

    // 1 Completed Audio (mp3, 10MB, 180s)
    repo.insert_download(&create_sample_record(
        "a1",
        "Audio 1",
        "mp3",
        DownloadStatus::Completed,
        180,
        10_000_000,
        1700000003,
    ))
    .unwrap();

    // 1 Failed Video (mkv, 0MB, 60s)
    repo.insert_download(&create_sample_record(
        "v3",
        "Video Failed",
        "mkv",
        DownloadStatus::Failed,
        60,
        0,
        1700000004,
    ))
    .unwrap();

    // 1 Cancelled Audio (flac, 5MB, 200s)
    repo.insert_download(&create_sample_record(
        "a2",
        "Audio Cancelled",
        "flac",
        DownloadStatus::Cancelled,
        200,
        5_000_000,
        1700000005,
    ))
    .unwrap();

    let stats = repo.get_statistics().unwrap();
    assert_eq!(stats.total_downloads, 5);
    assert_eq!(stats.completed_downloads, 3);
    assert_eq!(stats.failed_downloads, 1);
    assert_eq!(stats.cancelled_downloads, 1);
    assert_eq!(stats.total_downloaded_bytes, 115_000_000);
    assert_eq!(stats.audio_downloads, 2);
    assert_eq!(stats.video_downloads, 3);
    assert_eq!(stats.total_duration_seconds, 680);
}

#[test]
fn test_clear_history_operations() {
    let repo = create_in_memory_database().expect("Should create in-memory SQLite");

    repo.insert_download(&create_sample_record(
        "c1",
        "Done 1",
        "mp4",
        DownloadStatus::Completed,
        100,
        1000,
        1,
    ))
    .unwrap();
    repo.insert_download(&create_sample_record(
        "f1",
        "Fail 1",
        "mp4",
        DownloadStatus::Failed,
        100,
        0,
        2,
    ))
    .unwrap();
    repo.insert_download(&create_sample_record(
        "f2",
        "Fail 2",
        "mp4",
        DownloadStatus::Failed,
        100,
        0,
        3,
    ))
    .unwrap();

    // Clear failed only
    let cleared_failed = repo.clear_history(Some("failed")).unwrap();
    assert_eq!(cleared_failed, 2);

    let remaining = repo.get_history(&HistoryFilter::default()).unwrap();
    assert_eq!(remaining.total_count, 1);
    assert_eq!(remaining.items[0].id, "c1");

    // Clear all
    let cleared_all = repo.clear_history(None).unwrap();
    assert_eq!(cleared_all, 1);

    let final_check = repo.get_history(&HistoryFilter::default()).unwrap();
    assert_eq!(final_check.total_count, 0);
}

#[test]
fn test_export_history_json_and_csv() {
    let repo = create_in_memory_database().expect("Should create in-memory SQLite");

    let rec = create_sample_record(
        "exp1",
        "Title with, comma and \"quotes\" and \n newline",
        "mp3",
        DownloadStatus::Completed,
        210,
        8_500_000,
        1700000000,
    );
    repo.insert_download(&rec).unwrap();

    // JSON export
    let json_str = repo.export_history("json").unwrap();
    let parsed: Vec<DownloadRecord> = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].id, "exp1");
    assert_eq!(parsed[0].media_type(), "audio");

    // CSV export
    let csv_str = repo.export_history("csv").unwrap();
    assert!(csv_str.starts_with("id,source_url,service_name,title,duration"));
    assert!(csv_str.contains("\"Title with, comma and \"\"quotes\"\" and \n newline\""));
    assert!(csv_str.ends_with(",audio\r\n"));

    // Unsupported format returns error
    let err = repo.export_history("xml");
    assert!(err.is_err());
}
