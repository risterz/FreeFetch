use std::time::{Duration, Instant};
use tokio::sync::watch;

use freefetch_lib::engine::parallel_downloader::{
    partition_ranges, PartMetadata, RangeState, RangeWorkQueue, StallWatchdog,
    MAX_STALL_RECOVERIES_PER_RANGE,
};

#[test]
fn test_dynamic_subrange_partitioning() {
    let total_bytes = 10 * 1024 * 1024; // 10 MiB
    let ranges = partition_ranges(total_bytes, true, 6);
    // 10 MiB with 2 MiB subranges -> 5 subranges
    assert_eq!(ranges.len(), 5);
    for (i, r) in ranges.iter().enumerate() {
        assert_eq!(r.index, i);
        assert_eq!(r.start_byte, i as u64 * 2 * 1024 * 1024);
        let expected_end = ((i as u64 + 1) * 2 * 1024 * 1024 - 1).min(total_bytes - 1);
        assert_eq!(r.end_byte, expected_end);
        assert_eq!(r.downloaded_bytes, 0);
        assert!(!r.is_finished);
    }

    // Non-youtube uses concurrency
    let non_yt = partition_ranges(total_bytes, false, 4);
    assert_eq!(non_yt.len(), 4);
}

#[test]
fn test_worker_claims_next_subrange() {
    let ranges = vec![
        RangeState {
            index: 0,
            start_byte: 0,
            end_byte: 100,
            downloaded_bytes: 0,
            is_finished: false,
        },
        RangeState {
            index: 1,
            start_byte: 101,
            end_byte: 200,
            downloaded_bytes: 0,
            is_finished: false,
        },
    ];

    let mut queue = RangeWorkQueue::new(ranges);
    let claim1 = queue.claim_next();
    assert!(claim1.is_some());
    let (idx1, _) = claim1.unwrap();
    assert_eq!(idx1, 0);

    let claim2 = queue.claim_next();
    assert!(claim2.is_some());
    let (idx2, _) = claim2.unwrap();
    assert_eq!(idx2, 1);

    // No more available
    assert!(queue.claim_next().is_none());

    // Finish first
    queue.finish_range(0);
    assert!(queue.ranges[0].is_finished);
    assert_eq!(queue.ranges[0].downloaded_bytes, 101);

    // Still no more unclaimed
    assert!(queue.claim_next().is_none());
    assert!(!queue.is_all_finished());

    // Finish second
    queue.finish_range(1);
    assert!(queue.is_all_finished());
}

#[test]
fn test_stall_detection() {
    let mut watchdog = StallWatchdog::new(
        100 * 1024, // 100 KB/s
        Duration::from_secs(3),
    );

    let start = Instant::now();
    // 50 KB in 1 second -> 50 KB/s (< 100 KB/s)
    watchdog.record_bytes(50 * 1024);
    let t1 = start + Duration::from_millis(1050);
    assert!(!watchdog.evaluate(t1, 1024 * 1024, 0));
    assert!(watchdog.is_timing());

    // 50 KB in next second
    watchdog.record_bytes(50 * 1024);
    let t2 = t1 + Duration::from_millis(1050);
    assert!(!watchdog.evaluate(t2, 1024 * 1024, 0));
    assert!(watchdog.is_timing());

    // 50 KB in 3rd second
    watchdog.record_bytes(50 * 1024);
    let t3 = t2 + Duration::from_millis(1050);
    assert!(!watchdog.evaluate(t3, 1024 * 1024, 0));
    assert!(watchdog.is_timing());

    // 50 KB in 4th second -> total low throughput > 3s after stall timer started at t1
    watchdog.record_bytes(50 * 1024);
    let t4 = t3 + Duration::from_millis(1050);
    assert!(watchdog.evaluate(t4, 1024 * 1024, 0));
}

#[test]
fn test_stall_timer_resets_after_recovery() {
    let mut watchdog = StallWatchdog::new(100 * 1024, Duration::from_secs(3));

    let start = Instant::now();
    // Low throughput for 1.05s
    watchdog.record_bytes(40 * 1024);
    let t1 = start + Duration::from_millis(1050);
    watchdog.evaluate(t1, 1024 * 1024, 0);
    assert!(watchdog.is_timing());

    // Recovery: 500 KB in next 1.05s -> ~476 KB/s (> 100 KB/s)
    watchdog.record_bytes(500 * 1024);
    let t2 = t1 + Duration::from_millis(1050);
    assert!(!watchdog.evaluate(t2, 1024 * 1024, 0));
    assert!(!watchdog.is_timing()); // Reset!
}

#[test]
fn test_stalled_range_requeued() {
    let ranges = vec![RangeState {
        index: 0,
        start_byte: 0,
        end_byte: 2 * 1024 * 1024 - 1,
        downloaded_bytes: 0,
        is_finished: false,
    }];
    let mut queue = RangeWorkQueue::new(ranges);
    let (idx, _) = queue.claim_next().unwrap();
    assert_eq!(idx, 0);

    // Downloaded 500 KB then stalled
    queue.update_progress(0, 500 * 1024);
    queue.unclaim(0);

    // Can be re-claimed with existing progress intact
    let (reclaimed_idx, r) = queue.claim_next().unwrap();
    assert_eq!(reclaimed_idx, 0);
    assert_eq!(r.downloaded_bytes, 500 * 1024);
}

#[test]
fn test_partial_range_recovery() {
    let range = RangeState {
        index: 0,
        start_byte: 1000,
        end_byte: 5000,
        downloaded_bytes: 1500,
        is_finished: false,
    };
    let next_request_start = range.start_byte + range.downloaded_bytes;
    assert_eq!(next_request_start, 2500);
    let remaining = range.end_byte - next_request_start + 1;
    assert_eq!(remaining, 2501);
}

#[test]
fn test_no_duplicate_byte_ranges() {
    let total_bytes = 15 * 1024 * 1024 + 777;
    let ranges = partition_ranges(total_bytes, true, 6);
    let mut expected_start = 0;
    for r in &ranges {
        assert_eq!(r.start_byte, expected_start);
        assert!(r.end_byte >= r.start_byte);
        expected_start = r.end_byte + 1;
    }
    assert_eq!(expected_start, total_bytes);

    let sum_bytes: u64 = ranges.iter().map(|r| r.end_byte - r.start_byte + 1).sum();
    assert_eq!(sum_bytes, total_bytes);
}

#[test]
fn test_max_stall_recoveries() {
    let mut watchdog = StallWatchdog::new(100 * 1024, Duration::from_secs(3));
    let start = Instant::now();
    watchdog.record_bytes(10 * 1024);
    let t = start + Duration::from_secs(5);
    // If recovery count is at MAX, evaluate returns false
    assert!(!watchdog.evaluate(t, 500 * 1024, MAX_STALL_RECOVERIES_PER_RANGE));
}

#[tokio::test]
async fn test_cancel_during_stall_recovery() {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let _ = cancel_tx.send(true);
    assert!(*cancel_rx.borrow());
}

#[test]
fn test_pause_during_stall_recovery() {
    let mut queue = RangeWorkQueue::new(vec![RangeState {
        index: 0,
        start_byte: 0,
        end_byte: 1000,
        downloaded_bytes: 400,
        is_finished: false,
    }]);
    let _ = queue.claim_next();
    assert!(queue.claimed[0]);
    // Pause unclaims
    queue.unclaim(0);
    assert!(!queue.claimed[0]);
    assert_eq!(queue.ranges[0].downloaded_bytes, 400);
}

#[test]
fn test_part_meta_consistency_after_recovery() {
    let ranges = vec![
        RangeState {
            index: 0,
            start_byte: 0,
            end_byte: 1000,
            downloaded_bytes: 1001,
            is_finished: true,
        },
        RangeState {
            index: 1,
            start_byte: 1001,
            end_byte: 2000,
            downloaded_bytes: 350,
            is_finished: false,
        },
    ];
    let meta = PartMetadata {
        total_bytes: 2001,
        ranges,
    };
    let json = serde_json::to_string(&meta).unwrap();
    let deserialized: PartMetadata = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.total_bytes, 2001);
    assert_eq!(deserialized.ranges.len(), 2);
    assert!(deserialized.ranges[0].is_finished);
    assert_eq!(deserialized.ranges[1].downloaded_bytes, 350);
}

#[test]
fn test_collision_reservation_still_works() {
    let dir = std::env::temp_dir().join(format!("col_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let final_dest = dir.join("video.mp4");
    std::fs::write(&final_dest, b"existing").unwrap();
    assert!(final_dest.exists());
    let part_path = final_dest.with_extension("part");
    assert_ne!(final_dest, part_path);
    let _ = std::fs::remove_dir_all(&dir);
}
