use crate::config::AgentConfig;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use subtle::ConstantTimeEq;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Maximum downloaded core payload/archive size (`256 MiB`).
pub const MAX_CORE_BINARY_BYTES: usize = 256 * 1024 * 1024;
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCoreResult {
    pub sha256: String,
    pub detected_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    RawElf,
    Zip,
    TarGz,
    TarXz,
}

/// Supported proxy core binary targets for `/api/core-update` and `CoreUpdateCmd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreKind {
    Xray,
    Singbox,
    Naive,
    Mieru,
}

impl CoreKind {
    pub fn parse(core: &str) -> Result<Self, String> {
        match core.trim().to_ascii_lowercase().as_str() {
            "xray" | "xray-core" => Ok(Self::Xray),
            "sing-box" | "singbox" => Ok(Self::Singbox),
            "naive" | "naiveproxy" | "caddy-naive" => Ok(Self::Naive),
            "mieru" | "mita" => Ok(Self::Mieru),
            other => Err(format!("Unsupported core binary target: {other}")),
        }
    }

    pub fn canonical_name(&self) -> &'static str {
        match self {
            Self::Xray => "xray",
            Self::Singbox => "singbox",
            Self::Naive => "naive",
            Self::Mieru => "mieru",
        }
    }

    pub fn candidate_binary_names(&self) -> &'static [&'static str] {
        match self {
            Self::Xray => &["xray"],
            Self::Singbox => &["sing-box", "singbox"],
            Self::Naive => &["caddy", "caddy-naive", "naive"],
            Self::Mieru => &["mita", "mieru"],
        }
    }

    pub fn bin_path<'a>(&self, cfg: &'a AgentConfig) -> &'a PathBuf {
        match self {
            Self::Xray => &cfg.xray_bin,
            Self::Singbox => &cfg.singbox_bin,
            Self::Naive => &cfg.naive_bin,
            Self::Mieru => &cfg.mieru_bin,
        }
    }
}

/// Computes lowercase hex SHA-256 of `bytes` and verifies it in constant time against `expected_hex`.
pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> Result<String, String> {
    let expected = expected_hex.trim().to_ascii_lowercase();
    if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid expected SHA-256 hex digest (must be 64 hex chars)".into());
    }

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let actual = hex::encode(hasher.finalize());

    let eq = bool::from(actual.as_bytes().ct_eq(expected.as_bytes()));
    if !eq {
        return Err(format!(
            "SHA-256 digest mismatch: expected {expected}, got {actual}"
        ));
    }
    Ok(actual)
}

async fn atomic_write_bytes(
    target_path: &Path,
    bytes: &[u8],
    mode: u32,
    create_backup: bool,
) -> Result<(), String> {
    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
    }

    let file_name = target_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("core.bin");
    let tmp_path = target_path.with_file_name(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    let bak_path = backup_path(target_path);

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&tmp_path)
        .await
        .map_err(|e| format!("Failed to open temp file {}: {e}", tmp_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file
            .set_permissions(std::fs::Permissions::from_mode(mode))
            .await;
    }

    file.write_all(bytes)
        .await
        .map_err(|e| format!("Failed to write temp file {}: {e}", tmp_path.display()))?;
    file.sync_all()
        .await
        .map_err(|e| format!("Failed to sync temp file {}: {e}", tmp_path.display()))?;
    drop(file);

    if create_backup && tokio::fs::metadata(target_path).await.is_ok() {
        let _ = tokio::fs::copy(target_path, &bak_path).await;
    }

    if let Err(e) = tokio::fs::rename(&tmp_path, target_path).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(format!(
            "Failed to replace file {}: {e}",
            target_path.display()
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = tokio::fs::set_permissions(target_path, std::fs::Permissions::from_mode(mode)).await;
    }

    Ok(())
}

/// Verifies `expected_sha256`, backs up any existing binary to `<target>.bak`,
/// and atomically replaces `target_path` with `new_bytes` (mode `0755` on Unix).
pub async fn atomic_replace_binary(
    target_path: &Path,
    new_bytes: &[u8],
    expected_sha256: &str,
) -> Result<String, String> {
    let digest = verify_sha256(new_bytes, expected_sha256)?;
    atomic_write_bytes(target_path, new_bytes, 0o755, true).await?;
    Ok(digest)
}

/// Restores `<target_path>.bak` over `target_path` if a post-update check/restart fails.
pub async fn rollback_binary(target_path: &Path) -> Result<(), String> {
    let bak_path = backup_path(target_path);
    if tokio::fs::metadata(&bak_path).await.is_err() {
        return Err(format!("No backup binary found at {}", bak_path.display()));
    }
    tokio::fs::rename(&bak_path, target_path)
        .await
        .map_err(|e| format!("Failed to rollback binary {}: {e}", target_path.display()))
}

fn backup_path(target_path: &Path) -> PathBuf {
    let name = target_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("core");
    target_path.with_file_name(format!("{name}.bak"))
}

fn detect_archive_kind(download_url: &str, payload: &[u8]) -> Result<ArchiveKind, String> {
    if payload.starts_with(&ELF_MAGIC) {
        return Ok(ArchiveKind::RawElf);
    }
    if payload.starts_with(&[0x50, 0x4b, 0x03, 0x04]) {
        return Ok(ArchiveKind::Zip);
    }
    if payload.starts_with(&[0x1f, 0x8b]) {
        return Ok(ArchiveKind::TarGz);
    }
    if payload.starts_with(&[0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00]) {
        return Ok(ArchiveKind::TarXz);
    }
    let clean_url = download_url
        .split(['?', '#'])
        .next()
        .unwrap_or(download_url)
        .to_ascii_lowercase();
    if clean_url.ends_with(".zip") {
        return Ok(ArchiveKind::Zip);
    }
    if clean_url.ends_with(".tar.gz") || clean_url.ends_with(".tgz") {
        return Ok(ArchiveKind::TarGz);
    }
    if clean_url.ends_with(".tar.xz") {
        return Ok(ArchiveKind::TarXz);
    }
    Err("Downloaded payload is neither an ELF binary nor a recognized .zip/.tar.gz/.tar.xz archive".into())
}

struct TempDirGuard(PathBuf);
impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn run_extract_command(program: &str, args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    let out = tokio::time::timeout(Duration::from_secs(60), cmd.output())
        .await
        .map_err(|_| format!("{program} extraction timed out"))?
        .map_err(|e| format!("Failed to invoke {program}: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        return Err(format!(
            "{program} failed (exit {:?}): {} {}",
            out.status.code(),
            stderr.trim(),
            stdout.trim()
        ));
    }
    Ok(())
}

async fn walk_extracted_files(root: &Path, max_depth: usize) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0usize));

    while let Some((dir, depth)) = queue.pop_front() {
        let mut rd = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| format!("Failed to read extracted dir {}: {e}", dir.display()))?;
        while let Some(entry) = rd
            .next_entry()
            .await
            .map_err(|e| format!("Failed to iterate extracted dir {}: {e}", dir.display()))?
        {
            let path = entry.path();
            let Ok(meta) = tokio::fs::symlink_metadata(&path).await else {
                continue;
            };
            let ft = meta.file_type();
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() && depth < max_depth {
                queue.push_back((path, depth + 1));
            } else if ft.is_file() {
                files.push(path);
            }
        }
    }
    Ok(files)
}

async fn extract_core_payload(
    kind: CoreKind,
    download_url: &str,
    payload: &[u8],
) -> Result<(Vec<u8>, Vec<(String, Vec<u8>)>), String> {
    let archive_kind = detect_archive_kind(download_url, payload)?;
    if archive_kind == ArchiveKind::RawElf {
        return Ok((payload.to_vec(), Vec::new()));
    }

    let tmp_root = std::env::temp_dir().join(format!("mypanel-core-upd-{}", uuid::Uuid::new_v4()));
    let out_dir = tmp_root.join("out");
    tokio::fs::create_dir_all(&out_dir)
        .await
        .map_err(|e| format!("Failed to create extraction temp dir: {e}"))?;
    let _guard = TempDirGuard(tmp_root.clone());

    match archive_kind {
        ArchiveKind::Zip => {
            let zip_path = tmp_root.join("archive.zip");
            tokio::fs::write(&zip_path, payload)
                .await
                .map_err(|e| format!("Failed to write temp zip: {e}"))?;
            run_extract_command(
                "unzip",
                &[
                    "-o",
                    "-q",
                    &zip_path.to_string_lossy(),
                    "-d",
                    &out_dir.to_string_lossy(),
                ],
            )
            .await?;
        }
        ArchiveKind::TarGz => {
            let tar_path = tmp_root.join("archive.tar.gz");
            tokio::fs::write(&tar_path, payload)
                .await
                .map_err(|e| format!("Failed to write temp tar.gz: {e}"))?;
            run_extract_command(
                "tar",
                &[
                    "-xzf",
                    &tar_path.to_string_lossy(),
                    "-C",
                    &out_dir.to_string_lossy(),
                ],
            )
            .await?;
        }
        ArchiveKind::TarXz => {
            let tar_path = tmp_root.join("archive.tar.xz");
            tokio::fs::write(&tar_path, payload)
                .await
                .map_err(|e| format!("Failed to write temp tar.xz: {e}"))?;
            run_extract_command(
                "tar",
                &[
                    "-xJf",
                    &tar_path.to_string_lossy(),
                    "-C",
                    &out_dir.to_string_lossy(),
                ],
            )
            .await?;
        }
        ArchiveKind::RawElf => unreachable!(),
    }

    let files = walk_extracted_files(&out_dir, 4).await?;
    let candidates = kind.candidate_binary_names();

    let mut matched_bin_path: Option<PathBuf> = None;
    for candidate in candidates {
        if let Some(p) = files.iter().find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == *candidate)
        }) {
            matched_bin_path = Some(p.clone());
            break;
        }
    }

    let bin_path = matched_bin_path.ok_or_else(|| {
        format!(
            "Archive did not contain expected binary ({}) for {}",
            candidates.join(", "),
            kind.canonical_name()
        )
    })?;

    let bin_bytes = tokio::fs::read(&bin_path)
        .await
        .map_err(|e| format!("Failed to read extracted binary {}: {e}", bin_path.display()))?;
    if !bin_bytes.starts_with(&ELF_MAGIC) {
        return Err(format!(
            "Extracted binary {} is not a valid Linux ELF executable",
            bin_path.display()
        ));
    }

    let mut extra_assets = Vec::new();
    if kind == CoreKind::Xray {
        for asset_name in ["geoip.dat", "geosite.dat"] {
            if let Some(p) = files.iter().find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n == asset_name)
            }) {
                if let Ok(bytes) = tokio::fs::read(p).await {
                    if !bytes.is_empty() {
                        extra_assets.push((asset_name.to_string(), bytes));
                    }
                }
            }
        }
    }

    Ok((bin_bytes, extra_assets))
}

pub fn parse_version_output(kind: CoreKind, output: &str) -> Option<String> {
    let first_line = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    let token = match kind {
        CoreKind::Xray => first_line.split_whitespace().nth(1),
        CoreKind::Singbox => first_line.split_whitespace().nth(2),
        CoreKind::Naive | CoreKind::Mieru => first_line.split_whitespace().next(),
    }?;
    let clean = token.trim_start_matches(['v', 'V']).trim();
    if clean.is_empty() {
        None
    } else {
        Some(clean.to_string())
    }
}

async fn smoke_test_core_binary(kind: CoreKind, bin_path: &Path) -> Result<Option<String>, String> {
    let mut cmd = Command::new(bin_path);
    cmd.arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }

    let out = tokio::time::timeout(Duration::from_secs(5), cmd.output())
        .await
        .map_err(|_| format!("{} version check timed out", kind.canonical_name()))?
        .map_err(|e| format!("Failed to execute {} version check: {e}", bin_path.display()))?;

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() {
        return Err(format!(
            "{} exited with {:?}: {}",
            kind.canonical_name(),
            out.status.code(),
            combined.trim()
        ));
    }

    Ok(parse_version_output(kind, &combined))
}

/// Downloads a core archive or binary from `download_url`, verifies its SHA-256 digest,
/// extracts the target ELF binary, atomically installs it with `.bak` backup, and smoke-tests it
/// (rolling back on smoke-test failure).
pub async fn download_and_install_core(
    client: &reqwest::Client,
    kind: CoreKind,
    target_path: &Path,
    xray_asset_dir: Option<&Path>,
    download_url: &str,
    expected_sha256: &str,
) -> Result<UpdateCoreResult, String> {
    let parsed = url::Url::parse(download_url)
        .map_err(|e| format!("Invalid core download URL: {e}"))?;
    if !matches!(parsed.scheme(), "https" | "http") {
        return Err(format!("Unsupported download URL scheme: {}", parsed.scheme()));
    }

    let resp = client
        .get(parsed)
        .header(reqwest::header::USER_AGENT, "mypanel3-agent")
        .timeout(Duration::from_secs(180))
        .send()
        .await
        .map_err(|e| format!("Failed to download core package: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Core download failed with HTTP {}",
            resp.status()
        ));
    }
    if let Some(len) = resp.content_length() {
        if len as usize > MAX_CORE_BINARY_BYTES {
            return Err("Core package exceeds 256 MiB safety limit".into());
        }
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read core package response: {e}"))?;
    if bytes.len() > MAX_CORE_BINARY_BYTES {
        return Err("Core package exceeds 256 MiB safety limit".into());
    }

    let digest = verify_sha256(&bytes, expected_sha256)?;
    let (bin_bytes, extra_assets) = extract_core_payload(kind, download_url, &bytes).await?;

    atomic_write_bytes(target_path, &bin_bytes, 0o755, true).await?;

    let detected_version = match smoke_test_core_binary(kind, target_path).await {
        Ok(ver) => ver,
        Err(err) => {
            let _ = rollback_binary(target_path).await;
            return Err(format!(
                "Updated core binary failed verification (rolled back): {err}"
            ));
        }
    };

    if kind == CoreKind::Xray {
        if let Some(asset_dir) = xray_asset_dir {
            if let Err(err) = tokio::fs::create_dir_all(asset_dir).await {
                tracing::warn!(dir = %asset_dir.display(), error = %err, "Failed to create Xray asset dir");
            } else {
                for (name, data) in extra_assets {
                    let _ = atomic_write_bytes(&asset_dir.join(name), &data, 0o644, false).await;
                }
            }
        }
    }

    Ok(UpdateCoreResult {
        sha256: digest,
        detected_version,
    })
}

/// Legacy raw-binary helper preserved for compatibility.
pub async fn download_and_update_core(
    client: &reqwest::Client,
    target_path: &Path,
    download_url: &str,
    expected_sha256: &str,
) -> Result<String, String> {
    let kind = if target_path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.contains("sing"))
    {
        CoreKind::Singbox
    } else if target_path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.contains("caddy") || n.contains("naive"))
    {
        CoreKind::Naive
    } else if target_path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.contains("mita") || n.contains("mieru"))
    {
        CoreKind::Mieru
    } else {
        CoreKind::Xray
    };

    let res = download_and_install_core(client, kind, target_path, None, download_url, expected_sha256).await?;
    Ok(res.sha256)
}
