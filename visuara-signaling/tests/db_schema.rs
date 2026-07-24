//! Confirms `Db::open` can upgrade a database created by the pre-`role`
//! schema in place: opening it again shouldn't error, and existing rows
//! should read back as `AccountRole::User` via the `ALTER TABLE ... ADD
//! COLUMN` backed by `ensure_column`.

use visuara_signaling::db::{AccountRole, Db};

#[test]
fn upgrades_pre_role_schema_in_place() {
    let path = std::env::temp_dir().join(format!("visuara-test-schema-{}.db", uuid::Uuid::new_v4()));
    let path_str = path.to_str().unwrap().to_string();

    {
        // Simulate a database created by an older server version, before the
        // `role` column and `sessions` table existed.
        let conn = rusqlite::Connection::open(&path_str).expect("open raw connection");
        conn.execute_batch(
            "
            CREATE TABLE accounts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );
            INSERT INTO accounts (email, password_hash) VALUES ('old@example.com', 'hash');
            ",
        )
        .expect("create pre-upgrade schema");
    }

    // Opening via Db::open should upgrade the schema without erroring...
    let db = Db::open(&path_str).expect("open should upgrade schema in place");

    // ...and the pre-existing row should read back as a default `user` role.
    let account = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(db.find_account_by_email("old@example.com"))
        .expect("query should succeed")
        .expect("account should exist");
    assert_eq!(account.role, AccountRole::User);

    // Opening a second time (schema already upgraded) should also be a no-op, not an error.
    let db2 = Db::open(&path_str).expect("re-opening an already-upgraded db should succeed");
    let account2 = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(db2.find_account_by_email("old@example.com"))
        .expect("query should succeed")
        .expect("account should still exist");
    assert_eq!(account2.role, AccountRole::User);

    let _ = std::fs::remove_file(&path_str);
}
