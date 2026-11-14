use crate::error::AppResult;
use crate::models::*;
use crate::utils::{new_id, now_iso};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  source_url TEXT,
  platform TEXT,
  original_path TEXT,
  thumbnail_path TEXT,
  duration REAL NOT NULL DEFAULT 0,
  width INTEGER NOT NULL DEFAULT 0,
  height INTEGER NOT NULL DEFAULT 0,
  fps REAL NOT NULL DEFAULT 0,
  size_bytes INTEGER NOT NULL DEFAULT 0,
  video_codec TEXT,
  audio_codec TEXT,
  container TEXT,
  status TEXT NOT NULL DEFAULT 'ready',
  hashtags TEXT NOT NULL DEFAULT '[]',
  default_clip_seconds REAL NOT NULL DEFAULT 90,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS clips (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  part_index INTEGER NOT NULL,
  path TEXT NOT NULL,
  thumbnail_path TEXT,
  start_seconds REAL NOT NULL DEFAULT 0,
  end_seconds REAL NOT NULL DEFAULT 0,
  duration REAL NOT NULL DEFAULT 0,
  width INTEGER NOT NULL DEFAULT 0,
  height INTEGER NOT NULL DEFAULT 0,
  size_bytes INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'ready',
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_clips_project ON clips(project_id, part_index);

CREATE TABLE IF NOT EXISTS accounts (
  id TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  display_name TEXT NOT NULL,
  external_id TEXT NOT NULL,
  scopes TEXT NOT NULL DEFAULT '',
  token_blob BLOB NOT NULL,
  connected_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(platform, external_id)
);

CREATE TABLE IF NOT EXISTS secrets (
  key TEXT PRIMARY KEY,
  blob BLOB NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS publish_queue (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  clip_id TEXT NOT NULL,
  part_index INTEGER NOT NULL,
  platform TEXT NOT NULL,
  account_id TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  hashtags TEXT NOT NULL DEFAULT '[]',
  status TEXT NOT NULL DEFAULT 'pending',
  progress REAL NOT NULL DEFAULT 0,
  remote_id TEXT,
  remote_url TEXT,
  error TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  resume_data TEXT,
  sort_index INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_queue_order ON publish_queue(status, sort_index);

CREATE TABLE IF NOT EXISTS activity (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  level TEXT NOT NULL,
  message TEXT NOT NULL,
  detail TEXT,
  created_at TEXT NOT NULL
);
"#;

pub fn init(path: &Path) -> AppResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(8))?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

#[cfg(test)]
pub fn init_memory() -> AppResult<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

fn tags_to_json(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".into())
}

fn tags_from_json(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn project_from_row(row: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get("id")?,
        name: row.get("name")?,
        source_url: row.get("source_url")?,
        platform: row.get("platform")?,
        original_path: row.get("original_path")?,
        thumbnail_path: row.get("thumbnail_path")?,
        duration: row.get("duration")?,
        width: row.get("width")?,
        height: row.get("height")?,
        fps: row.get("fps")?,
        size_bytes: row.get("size_bytes")?,
        video_codec: row.get("video_codec")?,
        audio_codec: row.get("audio_codec")?,
        container: row.get("container")?,
        status: row.get("status")?,
        hashtags: tags_from_json(&row.get::<_, String>("hashtags")?),
        default_clip_seconds: row.get("default_clip_seconds")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        clip_count: row.get::<_, Option<i64>>("clip_count")?.unwrap_or(0),
        published_count: row.get::<_, Option<i64>>("published_count")?.unwrap_or(0),
    })
}

const PROJECT_SELECT: &str = r#"
SELECT p.*,
  (SELECT COUNT(*) FROM clips c WHERE c.project_id = p.id) AS clip_count,
  (SELECT COUNT(*) FROM publish_queue q WHERE q.project_id = p.id AND q.status = 'published') AS published_count
FROM projects p
"#;

pub fn insert_project(conn: &Connection, p: &Project) -> AppResult<()> {
    conn.execute(
        r#"INSERT INTO projects (id, name, source_url, platform, original_path, thumbnail_path,
            duration, width, height, fps, size_bytes, video_codec, audio_codec, container, status,
            hashtags, default_clip_seconds, created_at, updated_at)
           VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)"#,
        params![
            p.id,
            p.name,
            p.source_url,
            p.platform,
            p.original_path,
            p.thumbnail_path,
            p.duration,
            p.width,
            p.height,
            p.fps,
            p.size_bytes,
            p.video_codec,
            p.audio_codec,
            p.container,
            p.status,
            tags_to_json(&p.hashtags),
            p.default_clip_seconds,
            p.created_at,
            p.updated_at,
        ],
    )?;
    Ok(())
}

pub fn update_project(conn: &Connection, p: &Project) -> AppResult<()> {
    conn.execute(
        r#"UPDATE projects SET name=?2, source_url=?3, platform=?4, original_path=?5,
            thumbnail_path=?6, duration=?7, width=?8, height=?9, fps=?10, size_bytes=?11,
            video_codec=?12, audio_codec=?13, container=?14, status=?15, hashtags=?16,
            default_clip_seconds=?17, updated_at=?18 WHERE id=?1"#,
        params![
            p.id,
            p.name,
            p.source_url,
            p.platform,
            p.original_path,
            p.thumbnail_path,
            p.duration,
            p.width,
            p.height,
            p.fps,
            p.size_bytes,
            p.video_codec,
            p.audio_codec,
            p.container,
            p.status,
            tags_to_json(&p.hashtags),
            p.default_clip_seconds,
            p.updated_at,
        ],
    )?;
    Ok(())
}

pub fn get_project(conn: &Connection, id: &str) -> AppResult<Option<Project>> {
    let sql = format!("{PROJECT_SELECT} WHERE p.id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let row = stmt.query_row(params![id], project_from_row).optional()?;
    Ok(row)
}

pub fn list_projects(conn: &Connection) -> AppResult<Vec<Project>> {
    let sql = format!("{PROJECT_SELECT} ORDER BY p.updated_at DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], project_from_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn delete_project(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM clips WHERE project_id = ?1", params![id])?;
    conn.execute("DELETE FROM publish_queue WHERE project_id = ?1", params![id])?;
    conn.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
    Ok(())
}

fn clip_from_row(row: &Row<'_>) -> rusqlite::Result<Clip> {
    Ok(Clip {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        part_index: row.get("part_index")?,
        path: row.get("path")?,
        thumbnail_path: row.get("thumbnail_path")?,
        start_seconds: row.get("start_seconds")?,
        end_seconds: row.get("end_seconds")?,
        duration: row.get("duration")?,
        width: row.get("width")?,
        height: row.get("height")?,
        size_bytes: row.get("size_bytes")?,
        status: row.get("status")?,
        created_at: row.get("created_at")?,
    })
}

pub fn insert_clip(conn: &Connection, c: &Clip) -> AppResult<()> {
    conn.execute(
        r#"INSERT INTO clips (id, project_id, part_index, path, thumbnail_path, start_seconds,
            end_seconds, duration, width, height, size_bytes, status, created_at)
           VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"#,
        params![
            c.id,
            c.project_id,
            c.part_index,
            c.path,
            c.thumbnail_path,
            c.start_seconds,
            c.end_seconds,
            c.duration,
            c.width,
            c.height,
            c.size_bytes,
            c.status,
            c.created_at
        ],
    )?;
    Ok(())
}

pub fn list_clips(conn: &Connection, project_id: &str) -> AppResult<Vec<Clip>> {
    let mut stmt = conn.prepare("SELECT * FROM clips WHERE project_id = ?1 ORDER BY part_index ASC")?;
    let rows = stmt.query_map(params![project_id], clip_from_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn get_clip(conn: &Connection, id: &str) -> AppResult<Option<Clip>> {
    let mut stmt = conn.prepare("SELECT * FROM clips WHERE id = ?1")?;
    Ok(stmt.query_row(params![id], clip_from_row).optional()?)
}

pub fn delete_clip(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM clips WHERE id = ?1", params![id])?;
    conn.execute("DELETE FROM publish_queue WHERE clip_id = ?1", params![id])?;
    Ok(())
}

pub fn clear_clips(conn: &Connection, project_id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM clips WHERE project_id = ?1", params![project_id])?;
    Ok(())
}

fn account_from_row(row: &Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: row.get("id")?,
        platform: row.get("platform")?,
        display_name: row.get("display_name")?,
        external_id: row.get("external_id")?,
        scopes: row.get("scopes")?,
        connected_at: row.get("connected_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn upsert_account(
    conn: &Connection,
    platform: &str,
    display_name: &str,
    external_id: &str,
    scopes: &str,
    token_blob: &[u8],
) -> AppResult<Account> {
    let now = now_iso();
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM accounts WHERE platform = ?1 AND external_id = ?2",
            params![platform, external_id],
            |row| row.get(0),
        )
        .optional()?;
    let id = existing.unwrap_or_else(new_id);
    conn.execute(
        r#"INSERT INTO accounts (id, platform, display_name, external_id, scopes, token_blob, connected_at, updated_at)
           VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
           ON CONFLICT(id) DO UPDATE SET display_name=?3, scopes=?5, token_blob=?6, updated_at=?8"#,
        params![id, platform, display_name, external_id, scopes, token_blob, now, now],
    )?;
    Ok(Account {
        id,
        platform: platform.to_string(),
        display_name: display_name.to_string(),
        external_id: external_id.to_string(),
        scopes: scopes.to_string(),
        connected_at: now.clone(),
        updated_at: now,
    })
}

pub fn list_accounts(conn: &Connection) -> AppResult<Vec<Account>> {
    let mut stmt = conn.prepare("SELECT id, platform, display_name, external_id, scopes, connected_at, updated_at FROM accounts ORDER BY platform, display_name")?;
    let rows = stmt.query_map([], account_from_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn get_account(conn: &Connection, id: &str) -> AppResult<Option<Account>> {
    let mut stmt = conn.prepare("SELECT id, platform, display_name, external_id, scopes, connected_at, updated_at FROM accounts WHERE id = ?1")?;
    Ok(stmt.query_row(params![id], account_from_row).optional()?)
}

pub fn get_account_token_blob(conn: &Connection, id: &str) -> AppResult<Option<Vec<u8>>> {
    let mut stmt = conn.prepare("SELECT token_blob FROM accounts WHERE id = ?1")?;
    Ok(stmt.query_row(params![id], |row| row.get(0)).optional()?)
}

pub fn delete_account(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn set_secret(conn: &Connection, key: &str, blob: &[u8]) -> AppResult<()> {
    conn.execute(
        "INSERT INTO secrets (key, blob, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET blob=?2, updated_at=?3",
        params![key, blob, now_iso()],
    )?;
    Ok(())
}

pub fn get_secret(conn: &Connection, key: &str) -> AppResult<Option<Vec<u8>>> {
    let mut stmt = conn.prepare("SELECT blob FROM secrets WHERE key = ?1")?;
    Ok(stmt.query_row(params![key], |row| row.get(0)).optional()?)
}

pub fn delete_secret(conn: &Connection, key: &str) -> AppResult<()> {
    conn.execute("DELETE FROM secrets WHERE key = ?1", params![key])?;
    Ok(())
}

const QUEUE_SELECT: &str = r#"
SELECT q.*, COALESCE(a.display_name, '') AS account_name, COALESCE(p.name, '') AS project_name
FROM publish_queue q
LEFT JOIN accounts a ON a.id = q.account_id
LEFT JOIN projects p ON p.id = q.project_id
"#;

fn queue_from_row(row: &Row<'_>) -> rusqlite::Result<QueueItem> {
    Ok(QueueItem {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        clip_id: row.get("clip_id")?,
        project_name: row.get("project_name")?,
        part_index: row.get("part_index")?,
        platform: row.get("platform")?,
        account_id: row.get("account_id")?,
        account_name: row.get("account_name")?,
        title: row.get("title")?,
        description: row.get("description")?,
        hashtags: tags_from_json(&row.get::<_, String>("hashtags")?),
        status: row.get("status")?,
        progress: row.get("progress")?,
        remote_id: row.get("remote_id")?,
        remote_url: row.get("remote_url")?,
        error: row.get("error")?,
        attempts: row.get("attempts")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn insert_queue_item(conn: &Connection, item: &QueueItem, sort_index: i64) -> AppResult<()> {
    conn.execute(
        r#"INSERT INTO publish_queue (id, project_id, clip_id, part_index, platform, account_id,
            title, description, hashtags, status, progress, attempts, sort_index, created_at, updated_at)
           VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'pending',0,0,?10,?11,?12)"#,
        params![
            item.id,
            item.project_id,
            item.clip_id,
            item.part_index,
            item.platform,
            item.account_id,
            item.title,
            item.description,
            tags_to_json(&item.hashtags),
            sort_index,
            item.created_at,
            item.updated_at
        ],
    )?;
    Ok(())
}

pub fn list_queue(conn: &Connection) -> AppResult<Vec<QueueItem>> {
    let sql = format!("{QUEUE_SELECT} ORDER BY q.sort_index ASC, q.part_index ASC, q.created_at ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], queue_from_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn get_queue_item(conn: &Connection, id: &str) -> AppResult<Option<QueueItem>> {
    let sql = format!("{QUEUE_SELECT} WHERE q.id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    Ok(stmt.query_row(params![id], queue_from_row).optional()?)
}

pub fn next_pending_queue_item(conn: &Connection) -> AppResult<Option<QueueItem>> {
    let sql = format!(
        "{QUEUE_SELECT} WHERE q.status = 'pending' ORDER BY q.sort_index ASC, q.created_at ASC LIMIT 1"
    );
    let mut stmt = conn.prepare(&sql)?;
    Ok(stmt.query_row([], queue_from_row).optional()?)
}

pub fn queue_max_sort_index(conn: &Connection) -> AppResult<i64> {
    let value: Option<i64> = conn.query_row("SELECT MAX(sort_index) FROM publish_queue", [], |row| row.get(0))?;
    Ok(value.unwrap_or(0))
}

pub fn set_queue_status(conn: &Connection, id: &str, status: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE publish_queue SET status=?2, updated_at=?3 WHERE id=?1",
        params![id, status, now_iso()],
    )?;
    Ok(())
}

pub fn set_queue_progress(conn: &Connection, id: &str, progress: f64) -> AppResult<()> {
    conn.execute(
        "UPDATE publish_queue SET progress=?2, updated_at=?3 WHERE id=?1",
        params![id, progress.clamp(0.0, 100.0), now_iso()],
    )?;
    Ok(())
}

pub fn set_queue_error(conn: &Connection, id: &str, error: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE publish_queue SET status='error', error=?2, progress=0, updated_at=?3 WHERE id=?1",
        params![id, error, now_iso()],
    )?;
    Ok(())
}

pub fn set_queue_published(
    conn: &Connection,
    id: &str,
    remote_id: &str,
    remote_url: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE publish_queue SET status='published', progress=100, remote_id=?2, remote_url=?3,
         error=NULL, resume_data=NULL, updated_at=?4 WHERE id=?1",
        params![id, remote_id, remote_url, now_iso()],
    )?;
    Ok(())
}

pub fn set_queue_resume(conn: &Connection, id: &str, resume: Option<&str>) -> AppResult<()> {
    conn.execute(
        "UPDATE publish_queue SET resume_data=?2, updated_at=?3 WHERE id=?1",
        params![id, resume, now_iso()],
    )?;
    Ok(())
}

pub fn queue_resume_data(conn: &Connection, id: &str) -> AppResult<Option<String>> {
    let mut stmt = conn.prepare("SELECT resume_data FROM publish_queue WHERE id = ?1")?;
    let value: Option<Option<String>> = stmt.query_row(params![id], |row| row.get(0)).optional()?;
    Ok(value.flatten())
}

pub fn bump_queue_attempts(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE publish_queue SET attempts = attempts + 1, updated_at=?2 WHERE id=?1",
        params![id, now_iso()],
    )?;
    Ok(())
}

pub fn cancel_pending_items(conn: &Connection) -> AppResult<usize> {
    let changed = conn.execute(
        "UPDATE publish_queue SET status='canceled', updated_at=?1 WHERE status='pending'",
        params![now_iso()],
    )?;
    Ok(changed)
}

pub fn recover_interrupted_queue(conn: &Connection) -> AppResult<usize> {
    // Estados transitorios tras un cierre inesperado: vuelven a pendiente manteniendo
    // resume_data para que la subida se retome sin duplicar.
    let changed = conn.execute(
        "UPDATE publish_queue SET status='pending', progress=0, updated_at=?1
         WHERE status IN ('preparing','uploading')",
        params![now_iso()],
    )?;
    Ok(changed)
}

pub fn queue_counts(conn: &Connection) -> AppResult<(i64, i64, i64, i64, i64)> {
    let mut stmt = conn.prepare("SELECT status, COUNT(*) FROM publish_queue GROUP BY status")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
    let mut published = 0;
    let mut pending = 0;
    let mut failed = 0;
    let mut canceled = 0;
    let mut total = 0;
    for row in rows {
        let (status, count) = row?;
        total += count;
        match status.as_str() {
            "published" => published += count,
            "pending" | "preparing" | "uploading" => pending += count,
            "error" => failed += count,
            "canceled" => canceled += count,
            _ => {}
        }
    }
    Ok((published, pending, failed, canceled, total))
}

pub fn log_activity(conn: &Connection, level: &str, message: &str, detail: Option<&str>) -> AppResult<()> {
    conn.execute(
        "INSERT INTO activity (level, message, detail, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![level, message, detail, now_iso()],
    )?;
    Ok(())
}

pub fn recent_activity(conn: &Connection, limit: i64) -> AppResult<Vec<ActivityEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, level, message, detail, created_at FROM activity ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(ActivityEntry {
            id: row.get(0)?,
            level: row.get(1)?,
            message: row.get(2)?,
            detail: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[cfg(test)]
pub fn insert_queue_item_at_end(conn: &Connection, item: &QueueItem) -> AppResult<i64> {
    let sort = queue_max_sort_index(conn)? + 1;
    insert_queue_item(conn, item, sort)?;
    Ok(sort)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_project(id: &str) -> Project {
        Project {
            id: id.into(),
            name: "Proyecto de prueba".into(),
            created_at: now_iso(),
            updated_at: now_iso(),
            hashtags: vec!["viaje".into()],
            ..Default::default()
        }
    }

    #[test]
    fn project_roundtrip_with_counts() {
        let conn = init_memory().unwrap();
        let mut project = sample_project("p1");
        insert_project(&conn, &project).unwrap();
        let clip = Clip {
            id: "c1".into(),
            project_id: "p1".into(),
            part_index: 1,
            path: "C:/tmp/parte-01.mp4".into(),
            thumbnail_path: None,
            start_seconds: 0.0,
            end_seconds: 90.0,
            duration: 90.0,
            width: 1920,
            height: 1080,
            size_bytes: 100,
            status: "ready".into(),
            created_at: now_iso(),
        };
        insert_clip(&conn, &clip).unwrap();
        let loaded = get_project(&conn, "p1").unwrap().unwrap();
        assert_eq!(loaded.name, "Proyecto de prueba");
        assert_eq!(loaded.clip_count, 1);
        assert_eq!(loaded.hashtags, vec!["viaje"]);

        project.name = "Renombrado".into();
        project.updated_at = now_iso();
        update_project(&conn, &project).unwrap();
        assert_eq!(get_project(&conn, "p1").unwrap().unwrap().name, "Renombrado");
    }

    #[test]
    fn deleting_project_removes_clips_and_queue() {
        let conn = init_memory().unwrap();
        insert_project(&conn, &sample_project("p1")).unwrap();
        let item = QueueItem {
            id: "q1".into(),
            project_id: "p1".into(),
            clip_id: "c1".into(),
            project_name: String::new(),
            part_index: 1,
            platform: "youtube".into(),
            account_id: "a1".into(),
            account_name: String::new(),
            title: "t".into(),
            description: String::new(),
            hashtags: vec![],
            status: "pending".into(),
            progress: 0.0,
            remote_id: None,
            remote_url: None,
            error: None,
            attempts: 0,
            created_at: now_iso(),
            updated_at: now_iso(),
        };
        insert_queue_item(&conn, &item, 1).unwrap();
        delete_project(&conn, "p1").unwrap();
        assert!(get_project(&conn, "p1").unwrap().is_none());
        assert_eq!(list_queue(&conn).unwrap().len(), 0);
    }

    #[test]
    fn queue_is_processed_in_insertion_order() {
        let conn = init_memory().unwrap();
        insert_project(&conn, &sample_project("p1")).unwrap();
        for (index, part) in [3, 1, 2].iter().enumerate() {
            let item = QueueItem {
                id: format!("q{index}"),
                project_id: "p1".into(),
                clip_id: format!("c{part}"),
                project_name: String::new(),
                part_index: *part,
                platform: "youtube".into(),
                account_id: "a1".into(),
                account_name: String::new(),
                title: format!("parte {part}"),
                description: String::new(),
                hashtags: vec![],
                status: "pending".into(),
                progress: 0.0,
                remote_id: None,
                remote_url: None,
                error: None,
                attempts: 0,
                created_at: now_iso(),
                updated_at: now_iso(),
            };
            insert_queue_item_at_end(&conn, &item).unwrap();
        }
        let first = next_pending_queue_item(&conn).unwrap().unwrap();
        assert_eq!(first.part_index, 3);
        set_queue_status(&conn, &first.id, "published").unwrap();
        let second = next_pending_queue_item(&conn).unwrap().unwrap();
        assert_eq!(second.part_index, 1);
    }
}
