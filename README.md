
# Dir Permission Watcher

Dir Permission Watcher is a Rust-based application that monitors specified directories for permission changes and ensures they comply with desired permission settings.

## Features

- **Directory Monitoring**: Watches specified directories for changes.
- **Permission Enforcement**: Ensures files and directories have the desired permissions.
- **Configurable**: Easily configurable via a JSON file.

## Configuration

The application uses a configuration file located in the same directory as the executable. The configuration file can be automatically generated with default settings if it doesn't exist.

### Default Configuration
```json
{
    "watch_dirs": ["./testdir"],
    "ignore_dirs": ["./testdir/ignoreme"],
    "desired_permission": "777"
}
```

- `watch_dirs`: Directories to monitor.
- `ignore_dirs`: Directories to ignore.
- `desired_permission`: Desired permission mode (e.g., `777`).

## How to Run

1. **Clone the repository**:
    ```bash
    git clone https://github.com/MohamedBechirMejri/dir-permission-watcher.git
    ```
2. **Navigate to the project directory**:
    ```bash
    cd dir-permission-watcher
    ```
3. **Build the project**:
    ```bash
    cargo build --release
    ```
4. **Run the application**:
    ```bash
    ./target/release/dir-permission-watcher
    ```

## Dependencies

- Rust
- Tokio
- Notify
- Serde
- Tracing
- Walkdir

## Logging

The application uses `tracing` for logging. Initialize the logger at the start of the application.

## License

This project is licensed under the MIT License.

