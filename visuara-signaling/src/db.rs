//! SQLite-backed storage for accounts, sessions, and the device registry.
//! Queries run via spawn_blocking since rusqlite is synchronous, appropriate
//! at the self-hosted single-instance scale this server targets.
//!
//! No migration framework is used (no sqlx/refinery) — schema changes for
//! existing deployed databases go through `ensure_column`, a small
//! `pragma_table_info`-guarded `ALTER TABLE`, consistent with the
//! `CREATE TABLE IF NOT EXISTS` simplicity already used here.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountRole {
    User,
    Admin,
}

impl AccountRole {
    fn as_str(self) -> &'static str {
        match self {
            AccountRole::User => "user",
            AccountRole::Admin => "admin",
        }
    }

    fn from_column(s: &str) -> Self {
        match s {
            "admin" => AccountRole::Admin,
            _ => AccountRole::User,
        }
    }
}

pub struct Account {
    pub id: i64,
    pub email: String,
    pub password_hash: String,
    pub role: AccountRole,
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
    pub role: AccountRole,
}

fn ensure_column(conn: &Connection, table: &str, column: &str, ddl_type_and_default: &str) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
        params![table, column],
        |row| row.get::<_, i64>(0),
    )? > 0;
    if !exists {
        conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl_type_and_default}"), [])?;
    }
    Ok(())
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
            CREATE TABLE IF NOT EXISTS sessions (
                token TEXT PRIMARY KEY,
                account_id INTEGER NOT NULL REFERENCES accounts(id),
                created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );
            ",
        )?;
        // Upgrade path for databases created before the role column existed.
        ensure_column(&conn, "accounts", "role", "TEXT NOT NULL DEFAULT 'user'")?;
        Ok(Db(Arc::new(Mutex::new(conn))))
    }

    /// Admin-configured settings (e.g. the server URL embedded into
    /// downloadable clients, or the registration-enabled flag) — a simple
    /// key/value store since there's only ever one operator at this
    /// self-hosted scale.
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

    /// Whether public self-service registration is currently allowed.
    /// Absent means "not yet decided" (i.e. before the first account has
    /// ever been created), which should read as enabled.
    pub async fn registration_enabled(&self) -> Result<bool> {
        Ok(self.get_setting("registration_enabled").await?.map(|v| v == "true").unwrap_or(true))
    }

    pub async fn set_registration_enabled(&self, enabled: bool) -> Result<()> {
        self.set_setting("registration_enabled", if enabled { "true" } else { "false" }).await
    }

    pub async fn count_admins(&self) -> Result<i64> {
        let db = self.0.clone();
        tokio::task::spawn_blocking(move || -> Result<i64> {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT COUNT(*) FROM accounts WHERE role = 'admin'", [], |row| row.get(0))
                .map_err(Into::into)
        })
        .await?
    }

    pub async fn count_accounts(&self) -> Result<i64> {
        let db = self.0.clone();
        tokio::task::spawn_blocking(move || -> Result<i64> {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0)).map_err(Into::into)
        })
        .await?
    }

    pub async fn create_account(&self, email: &str, password_hash: &str, role: AccountRole) -> Result<i64> {
        let db = self.0.clone();
        let email = email.to_string();
        let password_hash = password_hash.to_string();
        tokio::task::spawn_blocking(move || -> Result<i64> {
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO accounts (email, password_hash, role) VALUES (?1, ?2, ?3)",
                params![email, password_hash, role.as_str()],
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
                "SELECT id, email, password_hash, role FROM accounts WHERE email = ?1",
                params![email],
                map_account_row,
            )
            .optional()
            .map_err(Into::into)
        })
        .await?
    }

    pub async fn find_account_by_id(&self, account_id: i64) -> Result<Option<Account>> {
        let db = self.0.clone();
        tokio::task::spawn_blocking(move || -> Result<Option<Account>> {
            let conn = db.lock().unwrap();
            conn.query_row(
                "SELECT id, email, password_hash, role FROM accounts WHERE id = ?1",
                params![account_id],
                map_account_row,
            )
            .optional()
            .map_err(Into::into)
        })
        .await?
    }

    pub async fn set_account_role(&self, account_id: i64, role: AccountRole) -> Result<()> {
        let db = self.0.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = db.lock().unwrap();
            conn.execute("UPDATE accounts SET role = ?1 WHERE id = ?2", params![role.as_str(), account_id])?;
            Ok(())
        })
        .await?
    }

    /// Deletes an account along with its devices and sessions (no
    /// `ON DELETE CASCADE` in this schema, so this is done explicitly in a
    /// transaction).
    pub async fn delete_account(&self, account_id: i64) -> Result<()> {
        let db = self.0.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut conn = db.lock().unwrap();
            let tx = conn.transaction()?;
            tx.execute("DELETE FROM sessions WHERE account_id = ?1", params![account_id])?;
            tx.execute("DELETE FROM devices WHERE account_id = ?1", params![account_id])?;
            tx.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
            tx.commit()?;
            Ok(())
        })
        .await?
    }

    pub async fn create_session(&self, token: &str, account_id: i64) -> Result<()> {
        let db = self.0.clone();
        let token = token.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = db.lock().unwrap();
            conn.execute("INSERT INTO sessions (token, account_id) VALUES (?1, ?2)", params![token, account_id])?;
            Ok(())
        })
        .await?
    }

    /// Resolves a session token to the account it belongs to, joining in one
    /// query so every authenticated request gets the current role without a
    /// second round trip.
    pub async fn find_session(&self, token: &str) -> Result<Option<Account>> {
        let db = self.0.clone();
        let token = token.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<Account>> {
            let conn = db.lock().unwrap();
            conn.query_row(
                "SELECT a.id, a.email, a.password_hash, a.role \
                 FROM sessions s JOIN accounts a ON a.id = s.account_id \
                 WHERE s.token = ?1",
                params![token],
                map_account_row,
            )
            .optional()
            .map_err(Into::into)
        })
        .await?
    }

    pub async fn delete_session(&self, token: &str) -> Result<()> {
        let db = self.0.clone();
        let token = token.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = db.lock().unwrap();
            conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])?;
            Ok(())
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
                map_device_row,
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
                .query_map(params![account_id], map_device_row)?
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
            let mut stmt = conn.prepare("SELECT id, email, created_at, role FROM accounts ORDER BY created_at")?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(AccountSummary {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        created_at: row.get(2)?,
                        role: AccountRole::from_column(&row.get::<_, String>(3)?),
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

fn map_account_row(row: &rusqlite::Row) -> rusqlite::Result<Account> {
    Ok(Account {
        id: row.get(0)?,
        email: row.get(1)?,
        password_hash: row.get(2)?,
        role: AccountRole::from_column(&row.get::<_, String>(3)?),
    })
}

fn map_device_row(row: &rusqlite::Row) -> rusqlite::Result<Device> {
    Ok(Device {
        id: row.get(0)?,
        account_id: row.get(1)?,
        name: row.get(2)?,
        unattended_password_hash: row.get(3)?,
    })
}
