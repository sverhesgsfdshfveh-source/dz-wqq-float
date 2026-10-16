//! 轻量错误日志：追加写 `%APPDATA%\wqq-float\log.txt`（排障用，失败静默）。

use std::io::Write;
use std::path::Path;

pub fn append_line(dir: &Path, msg: &str) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = std::fs::create_dir_all(dir);
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("log.txt"))
    else {
        return;
    };
    let _ = writeln!(f, "[{ts}] {msg}");
}
