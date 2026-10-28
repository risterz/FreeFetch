pub mod migrations;
pub mod repository;

use crate::core::error::FreeFetchError;
pub use repository::Repository;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub fn initialize_database<P: AsRef<Path>>(db_path: P) -> Result<Repository, FreeFetchError> {
    if let Some(parent) = db_path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn = Connection::open(db_path)?;
    migrations::run_migrations(&mut conn)?;
    let conn_arc = Arc::new(Mutex::new(conn));
    Ok(Repository::new(conn_arc))
}

pub fn create_in_memory_database() -> Result<Repository, FreeFetchError> {
    let mut conn = Connection::open_in_memory()?;
    migrations::run_migrations(&mut conn)?;
    let conn_arc = Arc::new(Mutex::new(conn));
    Ok(Repository::new(conn_arc))
}
