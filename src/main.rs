use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tokio::time;
use tracing::{error, info, warn};

use tokio::sync::mpsc;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    watch_dirs: Vec<String>,
    ignore_dirs: Vec<String>,
    desired_permission: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            watch_dirs: vec!["./testdir".to_string()],
            ignore_dirs: vec!["./testdir/ignoreme".to_string()],
            desired_permission: "777".to_string(),
        }
    }
}

impl Config {
    async fn load() -> io::Result<Self> {
        let exe_path = env::current_exe()?;
        let config_path = exe_path.parent().unwrap().join(".config");
        match fs::read_to_string(config_path.clone()) {
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
    async fn new(config: Config, event_tx: mpsc::Sender<()>) -> io::Result<Self> {
        let watcher = notify::recommended_watcher(move |res| match res {
            Ok(_) => {
                info!("File system event detected");
                if let Err(e) = event_tx.blocking_send(()) {
                    error!("Failed to send event notification: {}", e);
                }
            }
            Err(e) => error!("Watch error: {:?}", e),
        })
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(Self { config, watcher })
    }

    fn should_process_file(&self, path: &Path) -> bool {
        // Check if file is in ignored directories
        for dir in &self.config.ignore_dirs {
            if path.starts_with(dir) {
                // log ignored file and the directory that caused it
                info!(
                    "Ignoring file {} because it is in the ignored directory {}",
                    path.display(),
                    dir
                );
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

    // check if dirs are available
    for dir in &config.watch_dirs {
        if !Path::new(dir).exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Directory {} does not exist", dir),
            ));
        }
    }

    // Create a channel for watcher events
    let (event_tx, mut event_rx) = mpsc::channel(100);

    let mut checker = PermissionChecker::new(config, event_tx).await?;

    // Setup file watchers
    checker.setup_watchers().await?;

    // Run initial check
    checker.run_check().await?;

    // Setup file watchers
    checker.setup_watchers().await?;

    // Run initial check
    checker.run_check().await?;

    // Create interval for backup checks
    let mut interval = time::interval(Duration::from_secs(3600)); // 1 hour

    loop {
        tokio::select! {
            _ = interval.tick() => {
                info!("Running scheduled check");
                if let Err(e) = checker.run_check().await {
                    error!("Error during periodic check: {}", e);
                }
            }
            Some(_) = event_rx.recv() => {
                info!("Running check due to file system event");
                // Add a small delay to allow for multiple simultaneous events
                time::sleep(Duration::from_millis(100)).await;
                if let Err(e) = checker.run_check().await {
                    error!("Error during event-triggered check: {}", e);
                }
            }
        }
    }
}
