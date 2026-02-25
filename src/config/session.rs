//! Session persistence — save and load login credentials.

use std::fs;
use std::path::PathBuf;

/// Stored connection credentials.
#[derive(Clone)]
pub struct Session {
    pub host: String,
    pub port: String,
    pub user: String,
    pub password: String,
    pub dbname: String,
    pub schema: String,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: "5432".into(),
            user: "postgres".into(),
            password: String::new(),
            dbname: "postgres".into(),
            schema: "public".into(),
        }
    }
}

fn session_path() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pg_tables");
    let _ = fs::create_dir_all(&dir);
    dir.join("session.conf")
}

impl Session {
    /// Save current credentials to disk (plain text key=value).
    pub fn save(&self) {
        let content = format!(
            "host={}\nport={}\nuser={}\npassword={}\ndbname={}\nschema={}\n",
            self.host, self.port, self.user, self.password, self.dbname, self.schema
        );
        let _ = fs::write(session_path(), content);
    }

    /// Load credentials from disk. Returns `None` if file doesn't exist.
    pub fn load() -> Option<Self> {
        let content = fs::read_to_string(session_path()).ok()?;
        let mut session = Session::default();
        for line in content.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "host" => session.host = value.to_string(),
                    "port" => session.port = value.to_string(),
                    "user" => session.user = value.to_string(),
                    "password" => session.password = value.to_string(),
                    "dbname" => session.dbname = value.to_string(),
                    "schema" => session.schema = value.to_string(),
                    _ => {}
                }
            }
        }
        Some(session)
    }

    /// Delete saved session.
    pub fn clear() {
        let _ = fs::remove_file(session_path());
    }
}
