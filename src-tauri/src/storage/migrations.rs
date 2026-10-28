use rusqlite::Connection;

pub fn run_migrations(conn: &mut Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        PRAGMA synchronous = NORMAL;

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        ) WITHOUT ROWID;

        CREATE TABLE IF NOT EXISTS downloads (
            id TEXT PRIMARY KEY,
            source_url TEXT NOT NULL,
            service_name TEXT NOT NULL,
            title TEXT NOT NULL,
            duration INTEGER NOT NULL DEFAULT 0,
            thumbnail_url TEXT,
            destination_path TEXT NOT NULL,
            container TEXT NOT NULL,
            quality_label TEXT NOT NULL,
            total_bytes INTEGER DEFAULT 0,
            downloaded_bytes INTEGER DEFAULT 0,
            status TEXT NOT NULL,
            error_message TEXT,
            created_at INTEGER NOT NULL,
            completed_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS download_streams (
            id TEXT PRIMARY KEY,
            download_id TEXT NOT NULL,
            stream_type TEXT NOT NULL,
            format_id TEXT NOT NULL,
            codec TEXT,
            resolution TEXT,
            bitrate INTEGER,
            temp_file_path TEXT NOT NULL,
            total_bytes INTEGER DEFAULT 0,
            downloaded_bytes INTEGER DEFAULT 0,
            is_finished INTEGER DEFAULT 0,
            FOREIGN KEY(download_id) REFERENCES downloads(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);
        CREATE INDEX IF NOT EXISTS idx_downloads_created_at ON downloads(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_downloads_container ON downloads(container);
        CREATE INDEX IF NOT EXISTS idx_download_streams_download_id ON download_streams(download_id);

        CREATE TABLE IF NOT EXISTS queue_items (
            id TEXT PRIMARY KEY,
            url TEXT NOT NULL,
            canonical_url TEXT NOT NULL,
            title TEXT NOT NULL,
            thumbnail TEXT,
            status TEXT NOT NULL,
            selected_format TEXT NOT NULL,
            format_id TEXT NOT NULL,
            output_path TEXT NOT NULL,
            file_size INTEGER DEFAULT 0,
            downloaded_bytes INTEGER DEFAULT 0,
            progress REAL DEFAULT 0.0,
            speed INTEGER DEFAULT 0,
            eta INTEGER DEFAULT 0,
            error_code TEXT,
            error_message TEXT,
            retry_count INTEGER DEFAULT 0,
            created_at INTEGER NOT NULL,
            started_at INTEGER,
            completed_at INTEGER,
            updated_at INTEGER NOT NULL,
            position INTEGER NOT NULL,
            params_json TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_queue_status ON queue_items(status);
        CREATE INDEX IF NOT EXISTS idx_queue_position ON queue_items(position ASC);
        CREATE INDEX IF NOT EXISTS idx_queue_canonical_format ON queue_items(canonical_url, format_id);
        ",
    )?;

    // Crash recovery: Any job left in 'downloading' or 'processing' / 'remuxing' state resets to 'paused'
    conn.execute(
        "UPDATE downloads SET status = 'paused' WHERE status IN ('downloading', 'processing', 'remuxing')",
        [],
    )?;

    conn.execute(
        "UPDATE queue_items SET status = 'paused' WHERE status IN ('downloading', 'processing', 'preparing', 'retrying')",
        [],
    )?;

    Ok(())
}
