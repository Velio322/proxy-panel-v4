use crate::auth::crypto::{ct_eq, sha256_hex};
use crate::error::{AppError, AppResult};
use crate::services::core_updates;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;
use tokio::sync::Mutex;
use tokio_util::io::ReaderStream;

const EMBEDDED_INSTALL_SH: &str = include_str!("../../../../install.sh");
const EMBEDDED_NODE_UPGRADE_SH: &str = include_str!("../../../../deploy/node-upgrade.sh");

#[derive(Clone)]
struct CachedBinaryHash {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
    sha256: String,
}

static BINARY_HASH_CACHE: OnceLock<Mutex<Option<CachedBinaryHash>>> = OnceLock::new();

#[derive(Debug, Deserialize)]
pub struct BinaryQuery {
    pub arch: Option<String>,
}

#[cfg(unix)]
async fn override_is_trusted(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match tokio::fs::metadata(path).await {
        Ok(m) => m.uid() == 0 && m.mode() & 0o022 == 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
async fn override_is_trusted(_path: &Path) -> bool {
    false
}

async fn resolve_install_script(state: &AppState) -> String {
    // The data dir is writable by the panel user, so a compromised panel could otherwise plant
    // a script that every new node runs as root. Only a root-owned, non-group/world-writable
    // override is honored.
    let override_path = state.cfg.data_dir.join("install.sh");
    if override_is_trusted(&override_path).await {
        if let Ok(content) = tokio::fs::read_to_string(&override_path).await {
            if !content.trim().is_empty() {
                return content;
            }
        }
    }
    EMBEDDED_INSTALL_SH.to_string()
}

/// Serves the universal idempotent installer (`GET /install.sh`).
pub async fn install_sh(State(state): State<AppState>) -> impl IntoResponse {
    let script = resolve_install_script(&state).await;
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        script,
    )
}

/// Serves the in-place node upgrade script (`GET /node-upgrade.sh`). It reads the node's own
/// `/etc/mypanel3/node.env`, so the call carries no secret:
/// `curl -fsSL https://<panel>/node-upgrade.sh | sudo bash -s -- https://<panel>`.
pub async fn node_upgrade_sh() -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        EMBEDDED_NODE_UPGRADE_SH,
    )
}

/// Serves a node-focused entrypoint (`GET /install-node.sh`) defaulting to `--mode node`.
pub async fn install_node_sh(State(state): State<AppState>) -> impl IntoResponse {
    let base = resolve_install_script(&state).await;
    let body = base.strip_prefix("#!/usr/bin/env bash\n").unwrap_or(&base);
    let wrapped = format!(
        "#!/usr/bin/env bash\n\
         # ProxyPanel v4 — Dedicated Node Installer Wrapper (forces --mode node when omitted)\n\
         _has_mode=0\n\
         for _arg in \"$@\"; do\n\
           if [[ \"$_arg\" == \"--mode\" || \"$_arg\" == --mode=* ]]; then _has_mode=1; break; fi\n\
         done\n\
         if [[ $_has_mode -eq 0 ]]; then set -- --mode node \"$@\"; fi\n\
         unset _has_mode _arg\n\
         {body}"
    );
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        wrapped,
    )
}

async fn authenticate_any_node(state: &AppState, headers: &HeaderMap) -> AppResult<()> {
    let token = headers
        .get("x-node-secret")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .or_else(|| {
            headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| {
                    v.strip_prefix("Bearer ")
                        .or_else(|| v.strip_prefix("bearer "))
                })
                .map(str::trim)
        })
        .unwrap_or("");

    if token.len() < 16 || token.len() > 256 {
        return Err(AppError::unauthorized("Unauthorized"));
    }

    let secret_hash = sha256_hex(token);
    let rows = sqlx::query(
        r#"SELECT id, secret FROM "Node"
           WHERE active = true AND ("secretHash" = $1 OR secret = $2)
           LIMIT 2"#,
    )
    .bind(&secret_hash)
    .bind(token)
    .fetch_all(&state.db)
    .await
    .map_err(|_| AppError::service_unavailable("Authentication unavailable"))?;

    let matched = rows.into_iter().any(|r| {
        r.try_get::<String, _>("secret")
            .ok()
            .is_some_and(|s| ct_eq(token, &s))
    });

    if !matched {
        return Err(AppError::unauthorized("Unauthorized"));
    }

    Ok(())
}

async fn resolve_binary_path(state: &AppState, req_arch: &str) -> AppResult<PathBuf> {
    let data_candidate = state
        .cfg
        .data_dir
        .join("binaries")
        .join(format!("mypanel3-linux-{req_arch}"));
    if tokio::fs::try_exists(&data_candidate).await.unwrap_or(false) {
        return Ok(data_candidate);
    }

    let share_candidate = PathBuf::from(format!("/usr/local/share/mypanel3/mypanel3-linux-{req_arch}"));
    if tokio::fs::try_exists(&share_candidate).await.unwrap_or(false) {
        return Ok(share_candidate);
    }

    let host_arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };

    if req_arch == host_arch {
        let exe = std::env::current_exe().map_err(|e| {
            AppError::service_unavailable(format!("Unable to locate running mypanel3 binary: {e}"))
        })?;
        return Ok(exe);
    }

    Err(AppError::not_found(format!(
        "No prebuilt mypanel3 binary for '{req_arch}' (panel host runs on '{host_arch}'). Place mypanel3-linux-{req_arch} in {}/binaries or pass --binary-url.",
        state.cfg.data_dir.display()
    )))
}

async fn cached_file_sha256(path: &Path, len: u64, modified: Option<SystemTime>) -> AppResult<String> {
    let cache = BINARY_HASH_CACHE.get_or_init(|| Mutex::new(None));
    {
        let guard = cache.lock().await;
        if let Some(entry) = guard.as_ref() {
            if entry.path == path && entry.len == len && entry.modified == modified {
                return Ok(entry.sha256.clone());
            }
        }
    }

    let path_owned = path.to_path_buf();
    let digest = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        let mut f = std::fs::File::open(&path_owned)?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hex::encode(hasher.finalize()))
    })
    .await
    .map_err(|e| AppError::internal(format!("Failed to hash binary task: {e}")))?
    .map_err(|e| AppError::internal(format!("Failed to read mypanel3 binary: {e}")))?;

    let mut guard = cache.lock().await;
    *guard = Some(CachedBinaryHash {
        path: path.to_path_buf(),
        len,
        modified,
        sha256: digest.clone(),
    });
    Ok(digest)
}

/// Streams the authenticated node agent binary (`GET /api/v1/nodes/self/binary?arch=amd64|arm64`)
/// with `X-Binary-Sha256` verification header.
pub async fn download_node_binary(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<BinaryQuery>,
) -> AppResult<Response> {
    authenticate_any_node(&state, &headers).await?;

    let req_arch = core_updates::normalize_arch(q.arch.as_deref());
    let bin_path = resolve_binary_path(&state, req_arch).await?;

    let meta = tokio::fs::metadata(&bin_path)
        .await
        .map_err(|e| AppError::not_found(format!("Failed to stat mypanel3 binary: {e}")))?;
    let len = meta.len();
    let modified = meta.modified().ok();
    let sha256_hex_digest = cached_file_sha256(&bin_path, len, modified).await?;

    let file = tokio::fs::File::open(&bin_path)
        .await
        .map_err(|e| AppError::internal(format!("Failed to open mypanel3 binary: {e}")))?;
    let stream = ReaderStream::with_capacity(file, 64 * 1024);
    let body = Body::from_stream(stream);

    let mut resp = Response::new(body);
    *resp.status_mut() = StatusCode::OK;
    let hdrs = resp.headers_mut();
    hdrs.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    hdrs.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    if let Ok(cd) = HeaderValue::from_str(&format!(
        "attachment; filename=\"mypanel3-linux-{req_arch}\""
    )) {
        hdrs.insert(header::CONTENT_DISPOSITION, cd);
    }
    if let Ok(cl) = HeaderValue::from_str(&len.to_string()) {
        hdrs.insert(header::CONTENT_LENGTH, cl);
    }
    if let Ok(sha_hdr) = HeaderValue::from_str(&sha256_hex_digest) {
        hdrs.insert("x-binary-sha256", sha_hdr);
    }
    hdrs.insert("x-binary-arch", HeaderValue::from_static(req_arch));

    Ok(resp)
}
