//! Proves the SQLCipher link, per ADR-0005 and WP-02.
//!
//! Steps: open an encrypted database with a key, write a row, close, reopen
//! with the same key, read the row back. Then reopen with the wrong key and
//! confirm that reading fails.

use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

fn temp_db_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("sqlcipher-spike-{name}-{}.db", std::process::id()));
    path
}

fn open_keyed(path: &PathBuf, key: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    conn.pragma_update(None, "key", key)?;
    Ok(conn)
}

#[test]
fn write_close_reopen_with_correct_key_reads_back() {
    let path = temp_db_path("correct-key");
    let _ = std::fs::remove_file(&path);

    {
        let conn = open_keyed(&path, "correct horse battery staple").expect("open for write");
        conn.execute(
            "CREATE TABLE spike (id INTEGER PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .expect("create table under the cipher key");
        conn.execute(
            "INSERT INTO spike (value) VALUES (?1)",
            ["it works"],
        )
        .expect("insert row");
    } // conn dropped here, closing the connection

    let conn = open_keyed(&path, "correct horse battery staple").expect("reopen for read");
    let value: String = conn
        .query_row("SELECT value FROM spike WHERE id = 1", [], |row| row.get(0))
        .expect("read the row back through the same key");
    assert_eq!(value, "it works");

    drop(conn);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn reopen_with_wrong_key_fails_to_read() {
    let path = temp_db_path("wrong-key");
    let _ = std::fs::remove_file(&path);

    {
        let conn = open_keyed(&path, "correct horse battery staple").expect("open for write");
        conn.execute(
            "CREATE TABLE spike (id INTEGER PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .expect("create table under the cipher key");
        conn.execute("INSERT INTO spike (value) VALUES (?1)", ["it works"])
            .expect("insert row");
    }

    let conn = open_keyed(&path, "the wrong passphrase entirely").expect("open succeeds; SQLCipher does not verify the key until a page is read");
    let result: rusqlite::Result<String> =
        conn.query_row("SELECT value FROM spike WHERE id = 1", [], |row| row.get(0));

    assert!(
        result.is_err(),
        "reading with the wrong key must fail, not silently return data or an empty schema"
    );

    drop(conn);
    let _ = std::fs::remove_file(&path);
}
