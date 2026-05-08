use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::digest;
use rusqlite::{params, Connection};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("io error reading migrations: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("invalid system time")]
    InvalidSystemTime,
    #[error("migration file name has no version prefix: {name}")]
    InvalidFileName { name: String },
}

/// Bootstrap the `_skycode_migrations` ledger if it doesn't exist yet.
fn ensure_migrations_table(conn: &Connection) -> Result<(), MigrationError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _skycode_migrations (
            version    INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL,
            sha256     TEXT NOT NULL
        ) STRICT;",
    )?;
    Ok(())
}

/// Reads SQL files from `migrations_dir`, sorted by numeric version prefix,
/// and applies any that haven't been applied yet (tracked in `_skycode_migrations`).
pub fn run_migrations(conn: &Connection, migrations_dir: &Path) -> Result<u32, MigrationError> {
    let mut files = collect_migration_files(migrations_dir)?;
    files.sort_by_key(|(version, _, _)| *version);

    let mut applied_count: u32 = 0;

    for (version, sha256_hex, sql) in &files {
        let already_applied = if table_exists(conn, "_skycode_migrations")? {
            conn.prepare("SELECT 1 FROM _skycode_migrations WHERE version = ?1")?
                .exists(params![version])?
        } else {
            false
        };

        if already_applied {
            continue;
        }

        conn.execute_batch(sql)?;

        // If migrations did not create the ledger table, bootstrap it now.
        if !table_exists(conn, "_skycode_migrations")? {
            ensure_migrations_table(conn)?;
        }

        let now = now_unix()?;
        conn.execute(
            "INSERT INTO _skycode_migrations (version, applied_at, sha256) VALUES (?1, ?2, ?3)",
            params![version, now, sha256_hex],
        )?;

        applied_count += 1;
    }

    Ok(applied_count)
}

fn table_exists(conn: &Connection, table_name: &str) -> Result<bool, MigrationError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1")?;
    Ok(stmt.exists(params![table_name])?)
}

/// Collects `(version, sha256_hex, sql_content)` tuples from `.sql` files in the directory.
/// File names must start with a numeric prefix (e.g. `001_initial.sql`).
fn collect_migration_files(dir: &Path) -> Result<Vec<(i64, String, String)>, MigrationError> {
    let mut results = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| MigrationError::InvalidFileName {
                name: format!("{:?}", path),
            })?
            .to_string();

        let version = extract_version(&file_name)?;
        let sql = fs::read_to_string(&path)?;
        let sha256_hex = sha256_hex(sql.as_bytes());

        results.push((version, sha256_hex, sql));
    }

    Ok(results)
}

/// Extracts the leading integer from a file name like `001_initial.sql`.
fn extract_version(name: &str) -> Result<i64, MigrationError> {
    let prefix: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
    prefix
        .parse::<i64>()
        .map_err(|_| MigrationError::InvalidFileName {
            name: name.to_string(),
        })
}

fn sha256_hex(data: &[u8]) -> String {
    let hash = digest::digest(&digest::SHA256, data);
    let mut out = String::with_capacity(hash.as_ref().len() * 2);
    for b in hash.as_ref() {
        out.push(hex_char((b >> 4) & 0x0f));
        out.push(hex_char(b & 0x0f));
    }
    out
}

fn hex_char(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'a' + (value - 10)) as char,
    }
}

fn now_unix() -> Result<i64, MigrationError> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MigrationError::InvalidSystemTime)?
        .as_secs();
    i64::try_from(secs).map_err(|_| MigrationError::InvalidSystemTime)
}
