//! 自动升级：查 GitHub Release 最新版 → 下载新 exe → PowerShell 脚本替换并重启。
//!
//! 发布仓库（只放产物、不含源码）：https://github.com/sverhesgsfdshfveh-source/dz-wqq-float
//!
//! 流程：
//! 1. `check_update`：调 GitHub API `releases/latest`，用 tag 与编译期版本比较；
//! 2. `install_update`：下载 exe 到 %TEMP%（流式 + `update-progress` 事件），
//!    校验体积，然后以 `-EncodedCommand`（UTF-16LE base64，无脚本文件残留、
//!    无中文路径编码坑）启动一段 PowerShell：等本进程退出 → 旧→`.old`、新→原位
//!    （失败自动回滚）→ 重启新 exe；
//! 3. 本进程经 `quit_now` 退出（IPC 回调里直接 ExitProcess 会挂起，见 commands.rs 踩坑）。

use serde::Serialize;
use std::io::Write;
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const REPO: &str = "sverhesgsfdshfveh-source/dz-wqq-float";
/// 本仓库 release 附件下载地址前缀（install_update 只接受来自这里的 URL）
const ASSET_PREFIX: &str =
    "https://github.com/sverhesgsfdshfveh-source/dz-wqq-float/releases/download/";
/// 下载地址（手动兜底）——前端展示用
pub const RELEASES_PAGE: &str = "https://github.com/sverhesgsfdshfveh-source/dz-wqq-float/releases/latest";
/// 当前版本（编译期注入；Settings 页展示 + 版本比较基准）
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub size: u64,
    pub notes: String,
    /// 发布页（手动下载兜底）
    pub url: String,
    /// 新 exe 直链（交给 install_update）
    pub asset_url: String,
}

#[derive(serde::Deserialize)]
struct GhRelease {
    tag_name: String,
    body: Option<String>,
    html_url: Option<String>,
    assets: Vec<GhAsset>,
}

#[derive(serde::Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// "v1.2.3" / "1.2.3" / "1.2.3-beta" → (1,2,3)；非法格式 None
fn parse_ver(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next()?;
    let mut it = core.split('.');
    let a = it.next()?.trim().parse().ok()?;
    let b = it.next().unwrap_or("0").parse().ok()?;
    let c = it.next().unwrap_or("0").parse().ok()?;
    Some((a, b, c))
}

fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_ver(latest), parse_ver(current)), (Some(l), Some(c)) if l > c)
}

/// 标准 base64（PowerShell -EncodedCommand 用；避免为此引第三方依赖）
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = ((chunk[0] as u32) << 16) | ((b1 as u32) << 8) | b2 as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// -EncodedCommand 要求 UTF-16LE 字节的 base64
fn ps_encoded(cmd: &str) -> String {
    let bytes: Vec<u8> = cmd.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64(&bytes)
}

/// PowerShell 单引号字符串内的路径：' 需写成 ''
fn ps_quote(p: &Path) -> String {
    p.to_string_lossy().replace('\'', "''")
}

/// 替换脚本：等 pid 退出 → 旧文件挪 .old、新文件就位（失败回滚）→ 重启
fn replace_script(new_exe: &Path, old_exe: &Path, pid: u32) -> String {
    format!(
        "$ErrorActionPreference='SilentlyContinue'\n\
         $new='{new}'\n\
         $old='{old}'\n\
         $targetPid={pid}\n\
         while (Get-Process -Id $targetPid -ErrorAction SilentlyContinue) {{ Start-Sleep -Milliseconds 250 }}\n\
         Start-Sleep -Milliseconds 500\n\
         Move-Item -LiteralPath $old -Destination \"$old.old\" -Force\n\
         Move-Item -LiteralPath $new -Destination $old -Force\n\
         if (-not (Test-Path -LiteralPath $old)) {{\n\
           Move-Item -LiteralPath \"$old.old\" -Destination $old -Force\n\
           Set-Content -Path \"$env:TEMP\\wqq-float-update.log\" -Value 'replace failed' -Encoding UTF8\n\
           exit 1\n\
         }}\n\
         Remove-Item -LiteralPath \"$old.old\" -Force\n\
         Start-Process -FilePath $old\n",
        new = ps_quote(new_exe),
        old = ps_quote(old_exe),
        pid = pid
    )
}

/// 检查更新：GitHub 最新 Release vs 当前版本
#[tauri::command]
pub async fn check_update() -> Result<UpdateInfo, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("wqq-float/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let api = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = client
        .get(&api)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("网络连接失败（{e}）"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("发布仓库还没有 Release".into());
    }
    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        return Err("GitHub API 访问受限（稍后再试，或到发布页手动下载）".into());
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub 返回 {}", resp.status()));
    }
    let rel: GhRelease = resp.json().await.map_err(|e| format!("响应解析失败（{e}）"))?;
    let asset = rel
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("wqq-float.exe"))
        .or_else(|| {
            rel.assets
                .iter()
                .find(|a| a.name.to_ascii_lowercase().ends_with(".exe"))
        })
        .ok_or_else(|| "Release 里没有 exe 附件".to_string())?;
    Ok(UpdateInfo {
        current: VERSION.to_string(),
        latest: rel.tag_name.trim_start_matches(['v', 'V']).to_string(),
        has_update: is_newer(&rel.tag_name, VERSION),
        size: asset.size,
        notes: rel.body.unwrap_or_default(),
        url: rel.html_url.unwrap_or_else(|| RELEASES_PAGE.to_string()),
        asset_url: asset.browser_download_url.clone(),
    })
}

/// 下载并安装更新：成功后本进程会自动退出，由替换脚本重启新版本
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    asset_url: String,
    expected_size: u64,
) -> Result<(), String> {
    if !asset_url.starts_with(ASSET_PREFIX) {
        return Err("下载地址校验失败（仅允许本仓库 Release 附件）".into());
    }
    let exe = std::env::current_exe().map_err(|e| format!("定位程序失败（{e}）"))?;
    let dir = exe
        .parent()
        .ok_or("定位程序目录失败")?
        .to_path_buf();
    // 提前探测目录可写：避免下载几十 MB 之后才发现装不进去
    let probe = dir.join(".wqq-float-write-test");
    std::fs::write(&probe, b"x").map_err(|_| {
        "程序所在目录不可写：请把 wqq-float.exe 移到有写权限的位置（如桌面/文档）后重试，或到发布页手动下载"
            .to_string()
    })?;
    let _ = std::fs::remove_file(&probe);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .user_agent(concat!("wqq-float/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client
        .get(&asset_url)
        .send()
        .await
        .map_err(|e| format!("连接失败（{e}）"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败：HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(expected_size);
    let new_exe = std::env::temp_dir().join("wqq-float-new.exe");
    {
        let mut file = std::fs::File::create(&new_exe)
            .map_err(|e| format!("创建临时文件失败（{e}）"))?;
        let mut got: u64 = 0;
        let mut last_emit: u64 = 0;
        while let Some(chunk) = resp.chunk().await.map_err(|e| format!("下载中断（{e}）"))? {
            file.write_all(&chunk).map_err(|e| format!("写入失败（{e}）"))?;
            got += chunk.len() as u64;
            // 事件节流：每 256KB（或收尾）推一次进度
            if got - last_emit > 256 * 1024 || got == total {
                last_emit = got;
                let _ = app.emit(
                    "update-progress",
                    serde_json::json!({ "downloaded": got, "total": total }),
                );
            }
        }
        if expected_size > 0 && got != expected_size {
            let _ = std::fs::remove_file(&new_exe);
            return Err(format!("下载不完整（{got}/{expected_size} 字节），请重试"));
        }
        if got < 1024 * 1024 {
            let _ = std::fs::remove_file(&new_exe);
            return Err("下载文件异常（体积过小），请到发布页手动下载".into());
        }
    }

    // 启动替换脚本（独立进程，随本进程退出后接管）
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let script = replace_script(&new_exe, &exe, std::process::id());
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-EncodedCommand",
                &ps_encoded(&script),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("启动更新程序失败（{e}）"))?;
    }
    #[cfg(not(windows))]
    {
        return Err("自动更新仅支持 Windows".into());
    }

    // 退出（延迟退出，先落盘配置；见 quit_app 的踩坑：IPC 回调内直接 exit 会挂）
    crate::commands::quit_now(&app);
    Ok(())
}

/// 当前版本号（设置页展示）
#[tauri::command]
pub fn get_version() -> String {
    VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(parse_ver("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_ver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_ver("v0.1.0-beta.1"), Some((0, 1, 0)));
        assert_eq!(parse_ver("2.0"), Some((2, 0, 0)));
        assert_eq!(parse_ver("abc"), None);
        assert_eq!(parse_ver(""), None);
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.1.1", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.0.9", "0.1.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn base64_encodes_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn ps_encoded_is_utf16le_base64() {
        // [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes("echo 1")) 同款
        assert_eq!(ps_encoded("echo 1"), "ZQBjAGgAbwAgADEA");
    }

    #[test]
    fn ps_quote_escapes_single_quotes() {
        let p = Path::new("C:/it's/中文 目录/wqq-float.exe");
        assert_eq!(ps_quote(p), "C:/it''s/中文 目录/wqq-float.exe");
    }

    #[test]
    fn replace_script_waits_and_rolls_back() {
        let s = replace_script(
            Path::new(r"C:\Temp\wqq-float-new.exe"),
            Path::new(r"C:\App\wqq-float.exe"),
            4242,
        );
        assert!(s.contains("$targetPid=4242"));
        assert!(s.contains(r"C:\Temp\wqq-float-new.exe"));
        assert!(s.contains(r"C:\App\wqq-float.exe"));
        assert!(s.contains("Move-Item") && s.contains("Start-Process"));
        assert!(s.contains("rollback") || s.contains("$old.old"));
    }
}
