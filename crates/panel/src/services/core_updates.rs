use crate::error::{AppError, AppResult};
use crate::services::node_client;
use crate::state::AppState;
use mypanel_proto::CoreUpdateCmd;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::time::Duration;

const CACHE_TTL: Duration = Duration::from_secs(900);
const GITHUB_TIMEOUT: Duration = Duration::from_secs(20);
const USER_AGENT: &str = "mypanel3-core-updater";

#[derive(Debug, Clone, Default)]
pub struct ExtractedCoreInfo {
    pub xray: Option<String>,
    pub singbox: Option<String>,
    pub naive: Option<String>,
    pub mieru: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreReleaseAsset {
    pub core: String,
    pub name: String,
    pub latest_version: String,
    pub tag: String,
    pub arch: String,
    pub asset_name: String,
    pub download_url: String,
    pub sha256: String,
    pub release_url: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CoreUpdateOutcome {
    pub core: String,
    pub version: String,
    pub sha256: String,
    pub download_url: String,
    pub message: String,
}

struct CoreSpec {
    canonical: &'static str,
    display_name: &'static str,
    repo: &'static str,
}

const CORE_SPECS: [CoreSpec; 4] = [
    CoreSpec {
        canonical: "xray",
        display_name: "Xray-core",
        repo: "XTLS/Xray-core",
    },
    CoreSpec {
        canonical: "singbox",
        display_name: "Sing-box",
        repo: "SagerNet/sing-box",
    },
    CoreSpec {
        canonical: "naive",
        display_name: "NaiveProxy (Caddy)",
        repo: "klzgrad/forwardproxy",
    },
    CoreSpec {
        canonical: "mieru",
        display_name: "Mieru (mita)",
        repo: "enfein/mieru",
    },
];

pub fn normalize_core(raw: &str) -> AppResult<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "xray" | "xray-core" => Ok("xray"),
        "singbox" | "sing-box" => Ok("singbox"),
        "naive" | "naiveproxy" | "caddy-naive" => Ok("naive"),
        "mieru" | "mita" => Ok("mieru"),
        other => Err(AppError::bad_request(format!(
            "Unsupported proxy core: {other}"
        ))),
    }
}

pub fn normalize_arch(raw: Option<&str>) -> &'static str {
    match raw
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "arm64" | "aarch64" => "arm64",
        _ => "amd64",
    }
}

fn spec_for(canonical: &str) -> AppResult<&'static CoreSpec> {
    CORE_SPECS
        .iter()
        .find(|s| s.canonical == canonical)
        .ok_or_else(|| AppError::bad_request(format!("Unsupported proxy core: {canonical}")))
}

pub fn extract_core_info(cores: Option<&Value>) -> ExtractedCoreInfo {
    let Some(c) = cores else {
        return ExtractedCoreInfo::default();
    };
    let read_ver = |ptr: &str| -> Option<String> {
        c.pointer(ptr)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_start_matches(['v', 'V']).to_string())
    };
    let arch = c
        .pointer("/arch")
        .and_then(Value::as_str)
        .map(|a| normalize_arch(Some(a)).to_string());
    ExtractedCoreInfo {
        xray: read_ver("/xray/version"),
        singbox: read_ver("/singbox/version"),
        naive: read_ver("/naive/version"),
        mieru: read_ver("/mieru/version"),
        arch,
    }
}

fn normalize_sha256(raw: &str) -> Option<String> {
    let stripped = raw.trim().strip_prefix("sha256:").unwrap_or(raw.trim());
    let lower = stripped.to_ascii_lowercase();
    if lower.len() == 64 && lower.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(lower)
    } else {
        None
    }
}

fn find_hex64_in_line(line: &str) -> Option<String> {
    for part in line.split(|c: char| c.is_whitespace() || c == '=' || c == ':') {
        let trimmed = part.trim_matches(|c: char| !c.is_ascii_hexdigit());
        if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(trimmed.to_ascii_lowercase());
        }
    }
    None
}

fn parse_sha256_from_digest_text(text: &str) -> Option<String> {
    for line in text.lines() {
        if line.contains("256") {
            if let Some(hex) = find_hex64_in_line(line) {
                return Some(hex);
            }
        }
    }
    for line in text.lines() {
        if let Some(hex) = find_hex64_in_line(line) {
            return Some(hex);
        }
    }
    None
}

fn build_github_request(state: &AppState, url: &str, accept_json: bool) -> reqwest::RequestBuilder {
    let mut req = state
        .http_client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .timeout(GITHUB_TIMEOUT);
    if accept_json {
        req = req.header(reqwest::header::ACCEPT, "application/vnd.github+json");
    }
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            req = req.bearer_auth(trimmed);
        }
    }
    req
}

async fn github_get_json(state: &AppState, url: &str) -> Result<(reqwest::StatusCode, Value), AppError> {
    let resp = build_github_request(state, url, true)
        .send()
        .await
        .map_err(|e| AppError::service_unavailable(format!("GitHub API request failed: {e}")))?;
    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::service_unavailable(format!("GitHub API read failed: {e}")))?;
    let body: Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    Ok((status, body))
}

async fn github_get_text(state: &AppState, url: &str) -> AppResult<String> {
    let resp = build_github_request(state, url, false)
        .send()
        .await
        .map_err(|e| AppError::service_unavailable(format!("Checksum download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::service_unavailable(format!(
            "Checksum download returned HTTP {}",
            resp.status()
        )));
    }
    resp.text()
        .await
        .map_err(|e| AppError::service_unavailable(format!("Checksum read failed: {e}")))
}

async fn fetch_release_json(
    state: &AppState,
    repo: &str,
    requested_version: Option<&str>,
) -> AppResult<Value> {
    let req_ver = requested_version
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("latest"));

    if let Some(ver) = req_ver {
        let clean = ver.trim_start_matches(['v', 'V']);
        let with_v = format!("v{clean}");
        let url_with_v = format!("https://api.github.com/repos/{repo}/releases/tags/{with_v}");
        let (status, body) = github_get_json(state, &url_with_v).await?;
        if status.is_success() {
            return Ok(body);
        }
        if status == reqwest::StatusCode::NOT_FOUND && clean != with_v {
            let url_no_v = format!("https://api.github.com/repos/{repo}/releases/tags/{clean}");
            let (status2, body2) = github_get_json(state, &url_no_v).await?;
            if status2.is_success() {
                return Ok(body2);
            }
            if status2 == reqwest::StatusCode::NOT_FOUND {
                return Err(AppError::not_found(format!(
                    "Release tag '{ver}' not found in {repo}"
                )));
            }
            let msg = body2
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("GitHub API error");
            return Err(AppError::service_unavailable(format!(
                "GitHub API HTTP {status2} for {repo}: {msg}"
            )));
        }
        let msg = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("GitHub API error");
        return Err(AppError::service_unavailable(format!(
            "GitHub API HTTP {status} for {repo}: {msg}"
        )));
    }

    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let (status, body) = github_get_json(state, &url).await?;
    if !status.is_success() {
        let msg = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("GitHub API error");
        return Err(AppError::service_unavailable(format!(
            "GitHub API HTTP {status} for {repo}: {msg}"
        )));
    }
    Ok(body)
}

fn select_asset<'a>(
    core: &str,
    tag: &str,
    arch: &str,
    assets: &'a [Value],
) -> AppResult<&'a Value> {
    let ver = tag.trim_start_matches(['v', 'V']);
    let find_exact = |target: &str| {
        assets.iter().find(|a| {
            a.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n == target)
        })
    };

    let matched = match (core, arch) {
        ("xray", "amd64") => find_exact("Xray-linux-64.zip"),
        ("xray", "arm64") => find_exact("Xray-linux-arm64-v8a.zip"),
        ("singbox", _) => {
            let exact = format!("sing-box-{ver}-linux-{arch}.tar.gz");
            let suffix = format!("-linux-{arch}.tar.gz");
            find_exact(&exact).or_else(|| {
                assets.iter().find(|a| {
                    a.get("name").and_then(Value::as_str).is_some_and(|n| {
                        n.ends_with(&suffix)
                            && !n.contains("glibc")
                            && !n.contains("musl")
                            && !n.contains("legacy")
                    })
                })
            })
        }
        ("mieru", _) => {
            let exact = format!("mita_{ver}_linux_{arch}.tar.gz");
            let suffix = format!("_linux_{arch}.tar.gz");
            find_exact(&exact).or_else(|| {
                assets.iter().find(|a| {
                    a.get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|n| n.starts_with("mita_") && n.ends_with(&suffix))
                })
            })
        }
        ("naive", "amd64") => find_exact("caddy-forwardproxy-naive.tar.xz").or_else(|| {
            assets.iter().find(|a| {
                a.get("name").and_then(Value::as_str).is_some_and(|n| {
                    let l = n.to_ascii_lowercase();
                    l.contains("caddy")
                        && (l.ends_with(".tar.xz") || l.ends_with(".tar.gz") || l.ends_with(".zip"))
                        && !l.contains("arm64")
                        && !l.contains("aarch64")
                })
            })
        }),
        ("naive", "arm64") => {
            let found = assets.iter().find(|a| {
                a.get("name").and_then(Value::as_str).is_some_and(|n| {
                    let l = n.to_ascii_lowercase();
                    l.contains("caddy")
                        && (l.contains("arm64") || l.contains("aarch64"))
                        && (l.ends_with(".tar.xz") || l.ends_with(".tar.gz") || l.ends_with(".zip"))
                })
            });
            if found.is_none() {
                return Err(AppError::bad_request(
                    "klzgrad/forwardproxy does not publish prebuilt linux/arm64 binaries; pass custom downloadUrl and sha256",
                ));
            }
            found
        }
        _ => None,
    };

    matched.ok_or_else(|| {
        AppError::service_unavailable(format!(
            "No matching {arch} Linux release asset found for {core} ({tag})"
        ))
    })
}

async fn extract_asset_sha256(
    state: &AppState,
    asset: &Value,
    asset_name: &str,
    assets: &[Value],
) -> AppResult<String> {
    if let Some(raw) = asset.get("digest").and_then(Value::as_str) {
        if let Some(hex) = normalize_sha256(raw) {
            return Ok(hex);
        }
    }

    let companion_names = [
        format!("{asset_name}.dgst"),
        format!("{asset_name}.sha256.txt"),
        format!("{asset_name}.sha256"),
    ];
    for cname in companion_names {
        if let Some(comp) = assets.iter().find(|a| {
            a.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n == cname)
        }) {
            if let Some(url) = comp.get("browser_download_url").and_then(Value::as_str) {
                let text = github_get_text(state, url).await?;
                if let Some(hex) = parse_sha256_from_digest_text(&text) {
                    return Ok(hex);
                }
            }
        }
    }

    Err(AppError::service_unavailable(format!(
        "No SHA-256 digest available for release asset {asset_name}"
    )))
}

pub async fn resolve_release(
    state: &AppState,
    core: &str,
    requested_version: Option<&str>,
    arch: &str,
    force_refresh: bool,
) -> AppResult<CoreReleaseAsset> {
    let canonical = normalize_core(core)?;
    let spec = spec_for(canonical)?;
    let norm_arch = normalize_arch(Some(arch));
    let ver_key = requested_version
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("latest"))
        .map(|s| s.trim_start_matches(['v', 'V']).to_string())
        .unwrap_or_else(|| "latest".to_string());
    let cache_key = format!("core_release:{canonical}:{ver_key}:{norm_arch}");

    if !force_refresh {
        if let Some(cached) = state.cache.get_json(&cache_key) {
            if let Ok(parsed) = serde_json::from_value::<CoreReleaseAsset>(cached) {
                return Ok(parsed);
            }
        }
    }

    let rel = fetch_release_json(state, spec.repo, requested_version).await?;
    let tag = rel
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::service_unavailable(format!("Missing tag_name for {}", spec.repo)))?
        .to_string();
    let latest_version = tag.trim_start_matches(['v', 'V']).to_string();
    let release_url = rel
        .get("html_url")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("https://github.com/{}/releases/tag/{tag}", spec.repo));
    let published_at = rel
        .get("published_at")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let assets = rel
        .get("assets")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::service_unavailable(format!("Release assets missing for {}", spec.repo)))?;

    let asset = select_asset(canonical, &tag, norm_arch, assets)?;
    let asset_name = asset
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::service_unavailable("Release asset missing name"))?
        .to_string();
    let download_url = asset
        .get("browser_download_url")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::service_unavailable("Release asset missing download URL"))?
        .to_string();
    let sha256 = extract_asset_sha256(state, asset, &asset_name, assets).await?;

    let resolved = CoreReleaseAsset {
        core: canonical.to_string(),
        name: spec.display_name.to_string(),
        latest_version,
        tag,
        arch: norm_arch.to_string(),
        asset_name,
        download_url,
        sha256,
        release_url,
        published_at,
    };

    if let Ok(val) = serde_json::to_value(&resolved) {
        state.cache.set_json(cache_key, val, CACHE_TTL);
    }
    Ok(resolved)
}

pub async fn list_catalog(state: &AppState, arch: &str, force_refresh: bool) -> Vec<Value> {
    let norm_arch = normalize_arch(Some(arch));
    let catalog_cache_key = format!("core_catalog:{norm_arch}");
    if !force_refresh {
        if let Some(Value::Array(items)) = state.cache.get_json(&catalog_cache_key) {
            return items;
        }
    }

    let (r_xray, r_sb, r_naive, r_mieru) = tokio::join!(
        resolve_release(state, "xray", None, norm_arch, force_refresh),
        resolve_release(state, "singbox", None, norm_arch, force_refresh),
        resolve_release(state, "naive", None, norm_arch, force_refresh),
        resolve_release(state, "mieru", None, norm_arch, force_refresh),
    );

    let results = [
        (&CORE_SPECS[0], r_xray),
        (&CORE_SPECS[1], r_sb),
        (&CORE_SPECS[2], r_naive),
        (&CORE_SPECS[3], r_mieru),
    ];

    let mut out = Vec::with_capacity(4);
    let mut ok_count = 0usize;
    for (spec, res) in results {
        match res {
            Ok(rel) => {
                ok_count += 1;
                out.push(json!({
                    "core": rel.core,
                    "name": rel.name,
                    "latestVersion": rel.latest_version,
                    "tag": rel.tag,
                    "arch": rel.arch,
                    "assetName": rel.asset_name,
                    "sha256": rel.sha256,
                    "downloadUrl": rel.download_url,
                    "releaseUrl": rel.release_url,
                    "publishedAt": rel.published_at,
                    "recommended": true,
                }));
            }
            Err(err) => {
                tracing::warn!(core = spec.canonical, error = %err, "Failed to fetch upstream core release");
                out.push(json!({
                    "core": spec.canonical,
                    "name": spec.display_name,
                    "latestVersion": "",
                    "arch": norm_arch,
                    "releaseUrl": format!("https://github.com/{}/releases", spec.repo),
                    "error": err.to_string(),
                }));
            }
        }
    }

    if ok_count > 0 {
        state
            .cache
            .set_json(catalog_cache_key, Value::Array(out.clone()), CACHE_TTL);
    }
    out
}

async fn read_node_core_version(
    state: &AppState,
    node_id: &str,
    canonical_core: &str,
) -> Option<String> {
    let sql = match canonical_core {
        "xray" => r#"SELECT "xrayVersion" FROM "Node" WHERE id = $1"#,
        "singbox" => r#"SELECT "singboxVersion" FROM "Node" WHERE id = $1"#,
        "naive" => r#"SELECT "naiveVersion" FROM "Node" WHERE id = $1"#,
        "mieru" => r#"SELECT "mieruVersion" FROM "Node" WHERE id = $1"#,
        _ => return None,
    };
    sqlx::query_scalar::<_, Option<String>>(sql)
        .bind(node_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .flatten()
}

async fn write_node_core_version(
    state: &AppState,
    node_id: &str,
    canonical_core: &str,
    version: &str,
) {
    let sql = match canonical_core {
        "xray" => r#"UPDATE "Node" SET "xrayVersion" = $2, "updatedAt" = $3 WHERE id = $1"#,
        "singbox" => r#"UPDATE "Node" SET "singboxVersion" = $2, "updatedAt" = $3 WHERE id = $1"#,
        "naive" => r#"UPDATE "Node" SET "naiveVersion" = $2, "updatedAt" = $3 WHERE id = $1"#,
        "mieru" => r#"UPDATE "Node" SET "mieruVersion" = $2, "updatedAt" = $3 WHERE id = $1"#,
        _ => return,
    };
    let _ = sqlx::query(sql)
        .bind(node_id)
        .bind(version)
        .bind(crate::db::now())
        .execute(&state.db)
        .await;
}

pub async fn trigger_core_update(
    state: &AppState,
    node_id: &str,
    core: String,
    version: Option<String>,
    download_url: Option<String>,
    sha256: Option<String>,
) -> AppResult<CoreUpdateOutcome> {
    let core_norm = normalize_core(&core)?;
    let row = sqlx::query(r#"SELECT id, arch FROM "Node" WHERE id = $1"#)
        .bind(node_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("Node not found"))?;
    let arch_raw: Option<String> = row.try_get("arch").ok().flatten();
    let arch = normalize_arch(arch_raw.as_deref());

    let req_ver = version.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let req_url = download_url.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let req_sha = sha256.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let (resolved_ver, resolved_url, resolved_sha) = match (req_url, req_sha) {
        (Some(u), Some(s)) => {
            let clean_sha = normalize_sha256(s).ok_or_else(|| {
                AppError::bad_request("sha256 must be 64 hexadecimal characters")
            })?;
            let ver = req_ver
                .map(|v| v.trim_start_matches(['v', 'V']).to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "custom".to_string());
            (ver, u.to_string(), clean_sha)
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(AppError::bad_request(
                "Both downloadUrl and sha256 must be provided together",
            ));
        }
        (None, None) => {
            let rel = resolve_release(state, core_norm, req_ver, arch, false).await?;
            (rel.latest_version, rel.download_url, rel.sha256)
        }
    };

    let cmd = CoreUpdateCmd {
        command_id: format!("upd-{}", crate::db::new_id()),
        core: core_norm.to_string(),
        version: resolved_ver.clone(),
        download_url: resolved_url.clone(),
        sha256: resolved_sha.clone(),
    };

    let applied_ver = node_client::core_update(state, node_id, cmd).await?;

    let final_ver = if let Some(v) = applied_ver
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let clean = v.trim_start_matches(['v', 'V']).to_string();
        write_node_core_version(state, node_id, core_norm, &clean).await;
        clean
    } else if let Some(db_ver) = read_node_core_version(state, node_id, core_norm).await {
        db_ver
    } else {
        write_node_core_version(state, node_id, core_norm, &resolved_ver).await;
        resolved_ver
    };

    let spec = spec_for(core_norm)?;
    Ok(CoreUpdateOutcome {
        core: core_norm.to_string(),
        version: final_ver.clone(),
        sha256: resolved_sha,
        download_url: resolved_url,
        message: format!("{} updated to {}", spec.display_name, final_ver),
    })
}
