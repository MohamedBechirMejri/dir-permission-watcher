use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tokio::time;
use tracing::{error, info, warn};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    watch_dirs: Vec<String>,
    ignore_dirs: Vec<String>,
    desired_permission: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            watch_dirs: vec!["../testdir".to_string()],
            ignore_dirs: vec!["target".to_string()],
            desired_permission: "777".to_string(),
        }
    }
}

impl Config {
    async fn load() -> io::Result<Self> {
        let config_path = Path::new(".config");
        match fs::read_to_string(config_path) {
            Ok(content) => serde_json::from_str(&content).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Failed to parse config: {}", e),
                )
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let default = Self::default();
                let content = serde_json::to_string_pretty(&default)?;
                fs::write(config_path, content)?;
                Ok(default)
            }
            Err(e) => Err(e),
        }
    }
}

struct PermissionChecker {
    config: Config,
    watcher: RecommendedWatcher,
}

impl PermissionChecker {
    async fn new(config: Config) -> io::Result<Self> {
        let watcher = notify::recommended_watcher(|res| match res {
            Ok(event) => info!("File system event: {:?}", event),
            Err(e) => error!("Watch error: {:?}", e),
        })
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(Self { config, watcher })
    }

    fn should_process_file(&self, path: &Path) -> bool {
        // Check if file is in ignored directories
        for dir in &self.config.ignore_dirs {
            if path.starts_with(dir) {
                return false;
            }
        }
        true
    }

    async fn check_permissions(&self, dir: &str) -> io::Result<Vec<PathBuf>> {
        let mut files_with_wrong_permission = Vec::new();
        let desired_mode = u32::from_str_radix(&self.config.desired_permission, 8)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        for entry in walkdir::WalkDir::new(dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if self.should_process_file(path) {
                let metadata = fs::metadata(path)?;
                if metadata.permissions().mode() & 0o777 != desired_mode {
                    files_with_wrong_permission.push(path.to_path_buf());
                }
            }
        }

        Ok(files_with_wrong_permission)
    }

    async fn change_permissions(&self, files: Vec<PathBuf>) -> io::Result<()> {
        let desired_mode = u32::from_str_radix(&self.config.desired_permission, 8)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        for file in files {
            let mut perms = fs::metadata(&file)?.permissions();
            perms.set_mode(desired_mode);
            fs::set_permissions(&file, perms)?;
            info!(
                "Changed permissions of {} to {:o}",
                file.display(),
                desired_mode
            );
        }

        Ok(())
    }

    async fn setup_watchers(&mut self) -> io::Result<()> {
        for dir in &self.config.watch_dirs {
            let _ = self.watcher.watch(Path::new(dir), RecursiveMode::Recursive);
            info!("Watching directory: {}", dir);
        }
        Ok(())
    }

    async fn run_check(&self) -> io::Result<()> {
        for dir in &self.config.watch_dirs {
            match self.check_permissions(dir).await {
                Ok(files) => {
                    if !files.is_empty() {
                        self.change_permissions(files).await?;
                    }
                }
                Err(e) => {
                    warn!("Error checking permissions in {}: {}", dir, e);
                }
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load configuration
    let config = Config::load().await?;
    let mut checker = PermissionChecker::new(config).await?;

    // Setup file watchers
    checker.setup_watchers().await?;

    // Run initial check
    checker.run_check().await?;

    // Schedule periodic checks
    let mut interval = time::interval(Duration::from_secs(3600)); // 1 hour
    loop {
        interval.tick().await;
        if let Err(e) = checker.run_check().await {
            error!("Error during periodic check: {}", e);
        }
    }
}
