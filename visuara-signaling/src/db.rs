//! SQLite-backed storage for accounts and the device registry. Queries run
//! via spawn_blocking since rusqlite is synchronous, appropriate at the
//! self-hosted single-instance scale this server targets.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

pub struct Account {
    pub id: i64,
    pub password_hash: String,
}

pub struct Device {
    pub id: String,
    pub account_id: i64,
    pub name: String,
    pub unattended_password_hash: Option<String>,
}

pub struct AccountSummary {
    pub id: i64,
    pub email: String,
    pub created_at: i64,
}

impl Db {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS accounts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );
            CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                account_id INTEGER NOT NULL REFERENCES accounts(id),
                name TEXT NOT NULL,
                unattended_password_hash TEXT,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
        )?;
        Ok(Db(Arc::new(Mutex::new(conn))))
    }

    /// Admin-configured settings (e.g. the server URL embedded into
    /// downloadable clients) — a simple key/value store since there's only
    /// ever one operator at this self-hosted scale.
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let db = self.0.clone();
        let key = key.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<String>> {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| row.get(0))
                .optional()
                .map_err(Into::into)
        })
        .await?
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let db = self.0.clone();
        let key = key.to_string();
        let value = value.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn create_account(&self, email: &str, password_hash: &str) -> Result<i64> {
        let db = self.0.clone();
        let email = email.to_string();
        let password_hash = password_hash.to_string();
        tokio::task::spawn_blocking(move || -> Result<i64> {
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO accounts (email, password_hash) VALUES (?1, ?2)",
                params![email, password_hash],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await?
    }

    pub async fn find_account_by_email(&self, email: &str) -> Result<Option<Account>> {
        let db = self.0.clone();
        let email = email.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<Account>> {
            let conn = db.lock().unwrap();
            conn.query_row(
                "SELECT id, password_hash FROM accounts WHERE email = ?1",
                params![email],
                |row| {
                    Ok(Account {
                        id: row.get(0)?,
                        password_hash: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
        .await?
    }

    pub async fn create_device(&self, device_id: &str, account_id: i64, name: &str) -> Result<()> {
        let db = self.0.clone();
        let device_id = device_id.to_string();
        let name = name.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO devices (id, account_id, name) VALUES (?1, ?2, ?3)",
                params![device_id, account_id, name],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn find_device(&self, device_id: &str) -> Result<Option<Device>> {
        let db = self.0.clone();
        let device_id = device_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<Device>> {
            let conn = db.lock().unwrap();
            conn.query_row(
                "SELECT id, account_id, name, unattended_password_hash FROM devices WHERE id = ?1",
                params![device_id],
                |row| {
                    Ok(Device {
                        id: row.get(0)?,
                        account_id: row.get(1)?,
                        name: row.get(2)?,
                        unattended_password_hash: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
        .await?
    }

    pub async fn list_devices_for_account(&self, account_id: i64) -> Result<Vec<Device>> {
        let db = self.0.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<Device>> {
            let conn = db.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, account_id, name, unattended_password_hash FROM devices WHERE account_id = ?1",
            )?;
            let rows = stmt
                .query_map(params![account_id], |row| {
                    Ok(Device {
                        id: row.get(0)?,
                        account_id: row.get(1)?,
                        name: row.get(2)?,
                        unattended_password_hash: row.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await?
    }

    pub async fn set_unattended_password_hash(&self, device_id: &str, hash: &str) -> Result<()> {
        let db = self.0.clone();
        let device_id = device_id.to_string();
        let hash = hash.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = db.lock().unwrap();
            conn.execute(
                "UPDATE devices SET unattended_password_hash = ?1 WHERE id = ?2",
                params![hash, device_id],
            )?;
            Ok(())
        })
        .await?
    }

    /// For the admin accounts page — every registered account, newest last.
    pub async fn list_all_accounts(&self) -> Result<Vec<AccountSummary>> {
        let db = self.0.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<AccountSummary>> {
            let conn = db.lock().unwrap();
            let mut stmt = conn.prepare("SELECT id, email, created_at FROM accounts ORDER BY created_at")?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(AccountSummary {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        created_at: row.get(2)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await?
    }

    /// For the admin accounts page's device-delete action.
    pub async fn delete_device(&self, device_id: &str) -> Result<()> {
        let db = self.0.clone();
        let device_id = device_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = db.lock().unwrap();
            conn.execute("DELETE FROM devices WHERE id = ?1", params![device_id])?;
            Ok(())
        })
        .await?
    }
}
