use crate::core::error::FreeFetchError;
use crate::core::types::{
    DownloadRecord, DownloadStatistics, DownloadStatus, HistoryFilter, HistoryPage, QueueItem,
    QueueStatus, StreamRecord,
};
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Repository {
    conn: Arc<Mutex<Connection>>,
}

impl Repository {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub fn insert_download(&self, r: &DownloadRecord) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO downloads (
                id, source_url, service_name, title, duration, thumbnail_url,
                destination_path, container, quality_label, total_bytes,
                downloaded_bytes, status, error_message, created_at, completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                r.id,
                r.source_url,
                r.service_name,
                r.title,
                r.duration as i64,
                r.thumbnail_url,
                r.destination_path,
                r.container,
                r.quality_label,
                r.total_bytes as i64,
                r.downloaded_bytes as i64,
                r.status.as_str(),
                r.error_message,
                r.created_at,
                r.completed_at
            ],
        )?;
        Ok(())
    }

    pub fn update_download_progress(
        &self,
        id: &str,
        downloaded: u64,
        total: u64,
    ) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET downloaded_bytes = ?1, total_bytes = ?2 WHERE id = ?3",
            params![downloaded as i64, total as i64, id],
        )?;
        Ok(())
    }

    pub fn update_download_status(
        &self,
        id: &str,
        status: &DownloadStatus,
        error_message: Option<&str>,
        completed_at: Option<i64>,
    ) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET status = ?1, error_message = ?2, completed_at = COALESCE(?3, completed_at) WHERE id = ?4",
            params![status.as_str(), error_message, completed_at, id],
        )?;
        Ok(())
    }

    pub fn get_download(&self, id: &str) -> Result<Option<DownloadRecord>, FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, source_url, service_name, title, duration, thumbnail_url,
                    destination_path, container, quality_label, total_bytes,
                    downloaded_bytes, status, error_message, created_at, completed_at
             FROM downloads WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let duration_i64: i64 = row.get(4)?;
            let total_i64: i64 = row.get(9)?;
            let downloaded_i64: i64 = row.get(10)?;
            let status_str: String = row.get(11)?;

            Ok(Some(DownloadRecord {
                id: row.get(0)?,
                source_url: row.get(1)?,
                service_name: row.get(2)?,
                title: row.get(3)?,
                duration: duration_i64.max(0) as u64,
                thumbnail_url: row.get(5)?,
                destination_path: row.get(6)?,
                container: row.get(7)?,
                quality_label: row.get(8)?,
                total_bytes: total_i64.max(0) as u64,
                downloaded_bytes: downloaded_i64.max(0) as u64,
                status: DownloadStatus::parse(&status_str),
                error_message: row.get(12)?,
                created_at: row.get(13)?,
                completed_at: row.get(14)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_downloads(&self) -> Result<Vec<DownloadRecord>, FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, source_url, service_name, title, duration, thumbnail_url,
                    destination_path, container, quality_label, total_bytes,
                    downloaded_bytes, status, error_message, created_at, completed_at
             FROM downloads ORDER BY created_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            let duration_i64: i64 = row.get(4)?;
            let total_i64: i64 = row.get(9)?;
            let downloaded_i64: i64 = row.get(10)?;
            let status_str: String = row.get(11)?;

            Ok(DownloadRecord {
                id: row.get(0)?,
                source_url: row.get(1)?,
                service_name: row.get(2)?,
                title: row.get(3)?,
                duration: duration_i64.max(0) as u64,
                thumbnail_url: row.get(5)?,
                destination_path: row.get(6)?,
                container: row.get(7)?,
                quality_label: row.get(8)?,
                total_bytes: total_i64.max(0) as u64,
                downloaded_bytes: downloaded_i64.max(0) as u64,
                status: DownloadStatus::parse(&status_str),
                error_message: row.get(12)?,
                created_at: row.get(13)?,
                completed_at: row.get(14)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_download(&self, id: &str) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_active_destination_paths(
        &self,
        exclude_id: Option<&str>,
    ) -> Result<Vec<String>, FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let exclude = exclude_id.unwrap_or("");
        let mut stmt = conn.prepare(
            "SELECT destination_path FROM downloads WHERE id != ?1 AND status IN ('downloading', 'processing', 'retrying')
             UNION
             SELECT output_path FROM queue_items WHERE id != ?1 AND status IN ('queued', 'preparing', 'downloading', 'processing', 'retrying')",
        )?;
        let paths = stmt
            .query_map([exclude], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    pub fn get_history(&self, filter: &HistoryFilter) -> Result<HistoryPage, FreeFetchError> {
        let conn = self.conn.lock().unwrap();

        let mut conditions: Vec<String> = Vec::new();
        let mut param_values: Vec<rusqlite::types::Value> = Vec::new();

        if let Some(ref s) = filter.search {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                let pattern = format!("%{}%", trimmed.to_lowercase());
                conditions.push("(LOWER(title) LIKE ? OR LOWER(source_url) LIKE ?)".to_string());
                param_values.push(rusqlite::types::Value::Text(pattern.clone()));
                param_values.push(rusqlite::types::Value::Text(pattern));
            }
        }

        if let Some(ref st) = filter.status {
            let trimmed = st.trim().to_lowercase();
            if !trimmed.is_empty() && trimmed != "all" {
                if trimmed == "active" {
                    conditions.push("status IN ('queued', 'preparing', 'downloading', 'processing', 'retrying')".to_string());
                } else {
                    conditions.push("status = ?".to_string());
                    param_values.push(rusqlite::types::Value::Text(trimmed));
                }
            }
        }

        if let Some(ref mt) = filter.media_type {
            let mt_lower = mt.trim().to_lowercase();
            if mt_lower == "audio" {
                conditions.push(
                    "LOWER(container) IN ('mp3', 'm4a', 'flac', 'opus', 'aac', 'ogg', 'wav')"
                        .to_string(),
                );
            } else if mt_lower == "video" {
                conditions.push(
                    "LOWER(container) NOT IN ('mp3', 'm4a', 'flac', 'opus', 'aac', 'ogg', 'wav')"
                        .to_string(),
                );
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let count_sql = format!("SELECT COUNT(*) FROM downloads {where_clause}");
        let mut count_stmt = conn.prepare(&count_sql)?;
        let total_count: usize =
            count_stmt.query_row(rusqlite::params_from_iter(param_values.iter()), |r| {
                let count: i64 = r.get(0)?;
                Ok(count.max(0) as usize)
            })?;

        let limit = filter.limit.unwrap_or(50).clamp(1, 500);
        let offset = filter.offset.unwrap_or(0);

        let query_sql = format!(
            "SELECT id, source_url, service_name, title, duration, thumbnail_url,
                    destination_path, container, quality_label, total_bytes,
                    downloaded_bytes, status, error_message, created_at, completed_at
             FROM downloads {where_clause}
             ORDER BY created_at DESC
             LIMIT ? OFFSET ?"
        );

        let mut query_params = param_values;
        query_params.push(rusqlite::types::Value::Integer(limit as i64));
        query_params.push(rusqlite::types::Value::Integer(offset as i64));

        let mut stmt = conn.prepare(&query_sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(query_params.iter()), |row| {
            let duration_i64: i64 = row.get(4)?;
            let total_i64: i64 = row.get(9)?;
            let downloaded_i64: i64 = row.get(10)?;
            let status_str: String = row.get(11)?;

            Ok(DownloadRecord {
                id: row.get(0)?,
                source_url: row.get(1)?,
                service_name: row.get(2)?,
                title: row.get(3)?,
                duration: duration_i64.max(0) as u64,
                thumbnail_url: row.get(5)?,
                destination_path: row.get(6)?,
                container: row.get(7)?,
                quality_label: row.get(8)?,
                total_bytes: total_i64.max(0) as u64,
                downloaded_bytes: downloaded_i64.max(0) as u64,
                status: DownloadStatus::parse(&status_str),
                error_message: row.get(12)?,
                created_at: row.get(13)?,
                completed_at: row.get(14)?,
            })
        })?;

        let mut items = Vec::new();
        for r in rows {
            items.push(r?);
        }

        Ok(HistoryPage {
            items,
            total_count,
            limit,
            offset,
        })
    }

    pub fn get_statistics(&self) -> Result<DownloadStatistics, FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'cancelled' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(downloaded_bytes), 0),
                COALESCE(SUM(CASE WHEN LOWER(container) IN ('mp3', 'm4a', 'flac', 'opus', 'aac', 'ogg', 'wav') THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN LOWER(container) NOT IN ('mp3', 'm4a', 'flac', 'opus', 'aac', 'ogg', 'wav') THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(duration), 0)
             FROM downloads"
        )?;

        let stats = stmt.query_row([], |row| {
            let total: i64 = row.get(0)?;
            let completed: i64 = row.get(1)?;
            let failed: i64 = row.get(2)?;
            let cancelled: i64 = row.get(3)?;
            let bytes: i64 = row.get(4)?;
            let audio: i64 = row.get(5)?;
            let video: i64 = row.get(6)?;
            let duration: i64 = row.get(7)?;

            Ok(DownloadStatistics {
                total_downloads: total.max(0) as u64,
                completed_downloads: completed.max(0) as u64,
                failed_downloads: failed.max(0) as u64,
                cancelled_downloads: cancelled.max(0) as u64,
                total_downloaded_bytes: bytes.max(0) as u64,
                audio_downloads: audio.max(0) as u64,
                video_downloads: video.max(0) as u64,
                total_duration_seconds: duration.max(0) as u64,
            })
        })?;

        Ok(stats)
    }

    pub fn clear_history(&self, status_filter: Option<&str>) -> Result<usize, FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let rows_affected = match status_filter {
            Some(s) if !s.trim().is_empty() && s != "all" => {
                let trimmed = s.trim().to_lowercase();
                if trimmed == "active" {
                    conn.execute("DELETE FROM downloads WHERE status IN ('queued', 'preparing', 'downloading', 'processing', 'retrying')", [])?
                } else {
                    conn.execute("DELETE FROM downloads WHERE status = ?1", params![trimmed])?
                }
            }
            _ => conn.execute("DELETE FROM downloads", [])?,
        };
        Ok(rows_affected)
    }

    pub fn export_history(&self, format: &str) -> Result<String, FreeFetchError> {
        let items = self.list_downloads()?;
        match format.to_ascii_lowercase().as_str() {
            "json" => serde_json::to_string_pretty(&items)
                .map_err(|e| FreeFetchError::Generic(e.to_string())),
            "csv" => {
                let mut out = String::from(
                    "id,source_url,service_name,title,duration,thumbnail_url,destination_path,container,quality_label,total_bytes,downloaded_bytes,status,error_message,created_at,completed_at,media_type\r\n"
                );
                for it in &items {
                    let media_type = it.media_type();
                    let escape_csv = |s: &str| -> String {
                        if s.contains(',')
                            || s.contains('"')
                            || s.contains('\n')
                            || s.contains('\r')
                        {
                            format!("\"{}\"", s.replace('"', "\"\""))
                        } else {
                            s.to_string()
                        }
                    };

                    out.push_str(&format!(
                        "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\r\n",
                        escape_csv(&it.id),
                        escape_csv(&it.source_url),
                        escape_csv(&it.service_name),
                        escape_csv(&it.title),
                        it.duration,
                        escape_csv(it.thumbnail_url.as_deref().unwrap_or("")),
                        escape_csv(&it.destination_path),
                        escape_csv(&it.container),
                        escape_csv(&it.quality_label),
                        it.total_bytes,
                        it.downloaded_bytes,
                        escape_csv(it.status.as_str()),
                        escape_csv(it.error_message.as_deref().unwrap_or("")),
                        it.created_at,
                        it.completed_at.map(|c| c.to_string()).unwrap_or_default(),
                        media_type,
                    ));
                }
                Ok(out)
            }
            other => Err(FreeFetchError::Generic(format!(
                "Unsupported export format: {other}. Supported formats: json, csv"
            ))),
        }
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn upsert_stream(&self, s: &StreamRecord) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO download_streams (
                id, download_id, stream_type, format_id, codec, resolution,
                bitrate, temp_file_path, total_bytes, downloaded_bytes, is_finished
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(id) DO UPDATE SET
                downloaded_bytes = ?10,
                total_bytes = ?9,
                is_finished = ?11",
            params![
                s.id,
                s.download_id,
                s.stream_type,
                s.format_id,
                s.codec,
                s.resolution,
                s.bitrate.map(|b| b as i64),
                s.temp_file_path,
                s.total_bytes as i64,
                s.downloaded_bytes as i64,
                if s.is_finished { 1 } else { 0 },
            ],
        )?;
        Ok(())
    }

    pub fn get_stream(
        &self,
        download_id: &str,
        stream_type: &str,
    ) -> Result<Option<StreamRecord>, FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, download_id, stream_type, format_id, codec, resolution,
                    bitrate, temp_file_path, total_bytes, downloaded_bytes, is_finished
             FROM download_streams WHERE download_id = ?1 AND stream_type = ?2",
        )?;

        let mut rows = stmt.query(params![download_id, stream_type])?;
        if let Some(row) = rows.next()? {
            let bitrate_i64: Option<i64> = row.get(6)?;
            let total_i64: i64 = row.get(8)?;
            let downloaded_i64: i64 = row.get(9)?;
            let is_finished_i64: i64 = row.get(10)?;

            Ok(Some(StreamRecord {
                id: row.get(0)?,
                download_id: row.get(1)?,
                stream_type: row.get(2)?,
                format_id: row.get(3)?,
                codec: row.get(4)?,
                resolution: row.get(5)?,
                bitrate: bitrate_i64.map(|b| b.max(0) as u64),
                temp_file_path: row.get(7)?,
                total_bytes: total_i64.max(0) as u64,
                downloaded_bytes: downloaded_i64.max(0) as u64,
                is_finished: is_finished_i64 != 0,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn mark_stream_finished(
        &self,
        download_id: &str,
        stream_type: &str,
        finished: bool,
    ) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE download_streams SET is_finished = ?1 WHERE download_id = ?2 AND stream_type = ?3",
            params![if finished { 1 } else { 0 }, download_id, stream_type],
        )?;
        Ok(())
    }

    pub fn insert_queue_item(&self, item: &QueueItem) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO queue_items (
                id, url, canonical_url, title, thumbnail, status,
                selected_format, format_id, output_path, file_size,
                downloaded_bytes, progress, speed, eta, error_code,
                error_message, retry_count, created_at, started_at,
                completed_at, updated_at, position, params_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
            params![
                item.id,
                item.url,
                item.canonical_url,
                item.title,
                item.thumbnail,
                item.status.as_str(),
                item.selected_format,
                item.format_id,
                item.output_path,
                item.file_size as i64,
                item.downloaded_bytes as i64,
                item.progress,
                item.speed as i64,
                item.eta as i64,
                item.error_code,
                item.error_message,
                item.retry_count as i64,
                item.created_at,
                item.started_at,
                item.completed_at,
                item.updated_at,
                item.position,
                item.params_json
            ],
        )?;
        Ok(())
    }

    pub fn update_queue_item(&self, item: &QueueItem) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE queue_items SET
                status = ?1,
                file_size = ?2,
                downloaded_bytes = ?3,
                progress = ?4,
                speed = ?5,
                eta = ?6,
                error_code = ?7,
                error_message = ?8,
                retry_count = ?9,
                started_at = ?10,
                completed_at = ?11,
                updated_at = ?12,
                position = ?13
             WHERE id = ?14",
            params![
                item.status.as_str(),
                item.file_size as i64,
                item.downloaded_bytes as i64,
                item.progress,
                item.speed as i64,
                item.eta as i64,
                item.error_code,
                item.error_message,
                item.retry_count as i64,
                item.started_at,
                item.completed_at,
                item.updated_at,
                item.position,
                item.id
            ],
        )?;
        Ok(())
    }

    pub fn update_queue_item_status(
        &self,
        id: &str,
        status: &QueueStatus,
        error_code: Option<&str>,
        error_message: Option<&str>,
        started_at: Option<i64>,
        completed_at: Option<i64>,
    ) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE queue_items SET
                status = ?1,
                error_code = ?2,
                error_message = ?3,
                started_at = COALESCE(?4, started_at),
                completed_at = COALESCE(?5, completed_at),
                updated_at = ?6
             WHERE id = ?7",
            params![
                status.as_str(),
                error_code,
                error_message,
                started_at,
                completed_at,
                now,
                id
            ],
        )?;
        Ok(())
    }

    pub fn update_queue_item_progress(
        &self,
        id: &str,
        downloaded: u64,
        total: u64,
        progress: f64,
        speed: u64,
        eta: u64,
    ) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE queue_items SET
                downloaded_bytes = ?1,
                file_size = CASE WHEN ?2 > 0 THEN ?2 ELSE file_size END,
                progress = ?3,
                speed = ?4,
                eta = ?5,
                updated_at = ?6
             WHERE id = ?7",
            params![
                downloaded as i64,
                total as i64,
                progress,
                speed as i64,
                eta as i64,
                now,
                id
            ],
        )?;
        Ok(())
    }

    pub fn get_queue_item(&self, id: &str) -> Result<Option<QueueItem>, FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, canonical_url, title, thumbnail, status,
                    selected_format, format_id, output_path, file_size,
                    downloaded_bytes, progress, speed, eta, error_code,
                    error_message, retry_count, created_at, started_at,
                    completed_at, updated_at, position, params_json
             FROM queue_items WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(map_queue_item_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_queue_item_by_canonical(
        &self,
        canonical_url: &str,
        format_id: &str,
    ) -> Result<Option<QueueItem>, FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, canonical_url, title, thumbnail, status,
                    selected_format, format_id, output_path, file_size,
                    downloaded_bytes, progress, speed, eta, error_code,
                    error_message, retry_count, created_at, started_at,
                    completed_at, updated_at, position, params_json
             FROM queue_items WHERE canonical_url = ?1 AND format_id = ?2",
        )?;

        let mut rows = stmt.query(params![canonical_url, format_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(map_queue_item_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_queue_items(&self) -> Result<Vec<QueueItem>, FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, canonical_url, title, thumbnail, status,
                    selected_format, format_id, output_path, file_size,
                    downloaded_bytes, progress, speed, eta, error_code,
                    error_message, retry_count, created_at, started_at,
                    completed_at, updated_at, position, params_json
             FROM queue_items ORDER BY position ASC, created_at ASC",
        )?;

        let rows = stmt.query_map([], map_queue_item_row)?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_next_queued_items(&self, limit: usize) -> Result<Vec<QueueItem>, FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, canonical_url, title, thumbnail, status,
                    selected_format, format_id, output_path, file_size,
                    downloaded_bytes, progress, speed, eta, error_code,
                    error_message, retry_count, created_at, started_at,
                    completed_at, updated_at, position, params_json
             FROM queue_items WHERE status = 'queued' ORDER BY position ASC, created_at ASC LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], map_queue_item_row)?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_queue_item(&self, id: &str) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM queue_items WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear_completed_queue_items(&self) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM queue_items WHERE status = 'completed'", [])?;
        Ok(())
    }

    pub fn clear_failed_queue_items(&self) -> Result<(), FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM queue_items WHERE status IN ('failed', 'cancelled')",
            [],
        )?;
        Ok(())
    }

    pub fn get_max_queue_position(&self) -> Result<i32, FreeFetchError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT COALESCE(MAX(position), -1) FROM queue_items")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let max_pos: i32 = row.get(0)?;
            Ok(max_pos)
        } else {
            Ok(-1)
        }
    }

    pub fn update_queue_positions(
        &self,
        positions: &[(String, i32)],
    ) -> Result<(), FreeFetchError> {
        let mut conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        let tx = conn.transaction()?;
        for (id, pos) in positions {
            tx.execute(
                "UPDATE queue_items SET position = ?1, updated_at = ?2 WHERE id = ?3",
                params![pos, now, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn map_queue_item_row(row: &rusqlite::Row) -> rusqlite::Result<QueueItem> {
    let file_size_i64: i64 = row.get(9)?;
    let downloaded_i64: i64 = row.get(10)?;
    let speed_i64: i64 = row.get(12)?;
    let eta_i64: i64 = row.get(13)?;
    let retry_count_i64: i64 = row.get(16)?;
    let status_str: String = row.get(5)?;

    Ok(QueueItem {
        id: row.get(0)?,
        url: row.get(1)?,
        canonical_url: row.get(2)?,
        title: row.get(3)?,
        thumbnail: row.get(4)?,
        status: QueueStatus::parse(&status_str),
        selected_format: row.get(6)?,
        format_id: row.get(7)?,
        output_path: row.get(8)?,
        file_size: file_size_i64.max(0) as u64,
        downloaded_bytes: downloaded_i64.max(0) as u64,
        progress: row.get(11)?,
        speed: speed_i64.max(0) as u64,
        eta: eta_i64.max(0) as u64,
        error_code: row.get(14)?,
        error_message: row.get(15)?,
        retry_count: retry_count_i64.max(0) as u32,
        created_at: row.get(17)?,
        started_at: row.get(18)?,
        completed_at: row.get(19)?,
        updated_at: row.get(20)?,
        position: row.get(21)?,
        params_json: row.get(22)?,
    })
}
