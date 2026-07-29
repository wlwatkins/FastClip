//! SQLCipher link spike for ADR-0005.
//!
//! Standalone crate, not part of the application. Proves that `rusqlite`
//! with the `bundled-sqlcipher` feature can open an encrypted database,
//! write and read a row, and that the wrong key fails to read it back.
//! Evidence for the crate choice in ADR-0005; the decision itself is the
//! architect's.
