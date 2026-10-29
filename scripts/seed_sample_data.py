import sqlite3
import os
import time

db_path = os.path.expandvars(r'%APPDATA%\FreeFetch\freefetch.db')
print('Database path:', db_path)

conn = sqlite3.connect(db_path)
cur = conn.cursor()

now = int(time.time())

downloads = [
    (
        'dl-1',
        'https://www.youtube.com/watch?v=aqz-KE-bpKQ',
        'YouTube',
        'Big Buck Bunny (Official 1080p 60fps)',
        596,
        'https://picsum.photos/320/180?1',
        'C:\\Users\\FreeFetchUser\\Downloads\\BigBuckBunny_1080p.mp4',
        'mp4',
        '1080p',
        148897792,
        148897792,
        'completed',
        None,
        now - 3600,
        now - 3500,
    ),
    (
        'dl-2',
        'https://www.tiktok.com/@tech/video/123456789',
        'TikTok',
        'Rust vs Go Backend Architecture Breakdown',
        64,
        'https://picsum.photos/320/180?2',
        'C:\\Users\\FreeFetchUser\\Downloads\\Rust_vs_Go_Tech.mp4',
        'mp4',
        '720p',
        24117248,
        24117248,
        'completed',
        None,
        now - 7200,
        now - 7180,
    ),
    (
        'dl-3',
        'https://soundcloud.com/audio/cyberpunk-synth',
        'Generic',
        'Synthwave Nightdrive - Lossless Audio Master',
        240,
        'https://picsum.photos/320/180?3',
        'C:\\Users\\FreeFetchUser\\Downloads\\Synthwave_Nightdrive.mp3',
        'mp3',
        'MP3 320kbps',
        9680000,
        9680000,
        'completed',
        None,
        now - 10800,
        now - 10760,
    ),
]

for d in downloads:
    cur.execute(
        """
        INSERT OR REPLACE INTO downloads (
            id, source_url, service_name, title, duration, thumbnail_url,
            destination_path, container, quality_label, total_bytes, downloaded_bytes,
            status, error_message, created_at, completed_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        d,
    )

conn.commit()
print('Downloads count:', cur.execute('SELECT COUNT(*) FROM downloads').fetchone()[0])
conn.close()
