use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    watch_dirs: Vec<String>,
    ignore_dirs: Vec<String>,
    desired_permission: String,
}

impl Config {
    fn new() -> Self {
        let config = Config {
            watch_dirs: vec!["src".to_string(), "Cargo.toml".to_string()],
            ignore_dirs: vec!["target".to_string()],
            desired_permission: "777".to_string(),
        };
        config
    }
}

async fn load_config() -> Config {
    let config_file_path = Path::new(".config");
    let config = fs::read_to_string(config_file_path).await.unwrap();
    serde_json::from_str(&config).unwrap()
}

async fn check_permissions(dirs: Vec<String>, desired_permission: String) -> Vec<String> {
    let mut files_with_wrong_permission = vec![];
    for dir in dirs {
        let dir_path = Path::new(&dir);
        if dir_path.is_dir() {
            let dir_entries = fs::read_dir(dir_path).await.unwrap();
            for entry in dir_entries {
                let entry = entry.unwrap();
                let file_path = entry.path();
                let file_name = file_path.file_name().unwrap().to_str().unwrap();
                if file_name.ends_with(".rs") {
                    let file_permission = fs::metadata(file_path).await.unwrap().permissions();
                    if file_permission.mode()
                        != u32::from_str_radix(&desired_permission, 8).unwrap()
                    {
                        files_with_wrong_permission.push(file_name.to_string());
                    }
                }
            }
        }
    }
    files_with_wrong_permission
}

async fn change_permissions(files: Vec<String>, desired_permission: String) {
    for file in files {
        fs::set_permissions(
            Path::new(&file),
            fs::Permissions::from_mode_str(&desired_permission).unwrap(),
        )
        .await
        .unwrap();
        println!("Changed permission of {}", file);
    }
}

async fn run(config: Config) {
    let files_with_wrong_permission =
        check_permissions(config.watch_dirs, config.desired_permission).await;
    change_permissions(files_with_wrong_permission, config.desired_permission).await;
}

async fn main() {
    let config = load_config().await;

    let interval = tokio::time::interval(std::time::Duration::from_hours(1));
    loop {
        run(config.clone()).await;
        interval.tick().await;
    }
}
