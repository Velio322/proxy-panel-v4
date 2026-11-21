use crate::auth::crypto::sha256_hex;
use crate::auth::password::hash_password;
use crate::db::enums::{NodeStatus, PlanType, UserRole};
use anyhow::Result;
use sqlx::PgPool;

/// Seeds default super-admin user and baseline plans if none exist yet.
pub async fn seed_initial_data(pool: &PgPool) -> Result<()> {
    let user_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "User""#)
        .fetch_one(pool)
        .await?;

    if user_count == 0 {
        let username = std::env::var("ADMIN_USERNAME")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "admin".to_string());
        let password = std::env::var("ADMIN_PASSWORD")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "admin123".to_string());

        let pw_hash = hash_password(&password, 12).await?;
        let id = crate::db::new_id();
        let now = crate::db::now();

        sqlx::query(
            r#"INSERT INTO "User" (id, username, password, role, language, banned, "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, 'en', false, $5, $5)
               ON CONFLICT (username) DO NOTHING"#,
        )
        .bind(id)
        .bind(&username)
        .bind(pw_hash)
        .bind(UserRole::SuperAdmin)
        .bind(now)
        .execute(pool)
        .await?;

        tracing::info!(username = %username, "Seeded initial SUPER_ADMIN user");
    }

    let plan_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Plan""#)
        .fetch_one(pool)
        .await?;

    if plan_count == 0 {
        let now = crate::db::now();
        let plans = [
            (
                "Starter 30d",
                "50 GB / 30 days",
                30i32,
                50i64 * 1024 * 1024 * 1024,
                serde_json::json!(["VLESS", "HYSTERIA2"]),
                1i32,
            ),
            (
                "Pro 30d",
                "200 GB / 30 days",
                30i32,
                200i64 * 1024 * 1024 * 1024,
                serde_json::json!(["VLESS", "VMESS", "TROJAN", "HYSTERIA2"]),
                2i32,
            ),
        ];

        for (name, desc, duration, traffic_limit, protocols, sort_order) in plans {
            sqlx::query(
                r#"INSERT INTO "Plan" (id, name, description, type, price, currency, duration, "trafficLimit", protocols, "sortOrder", active, "createdAt", "updatedAt")
                   VALUES ($1, $2, $3, $4, 0, 'USD', $5, $6, $7, $8, true, $9, $9)"#,
            )
            .bind(crate::db::new_id())
            .bind(name)
            .bind(desc)
            .bind(PlanType::User)
            .bind(duration)
            .bind(traffic_limit)
            .bind(protocols)
            .bind(sort_order)
            .bind(now)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

/// Creates or updates a `SUPER_ADMIN` account for CLI `mypanel3 admin create`.
/// Returns `Ok(true)` when created/updated, or `Ok(false)` when `if_missing` is true and an admin already exists.
pub async fn create_admin(
    pool: &PgPool,
    username: &str,
    password: &str,
    email: Option<&str>,
    if_missing: bool,
) -> Result<bool> {
    if if_missing {
        let existing_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "User" WHERE role = 'SUPER_ADMIN'"#,
        )
        .fetch_one(pool)
        .await?;
        if existing_count > 0 {
            return Ok(false);
        }
    }

    let pw_hash = hash_password(password, 12).await?;
    let id = crate::db::new_id();
    let now = crate::db::now();

    sqlx::query(
        r#"INSERT INTO "User" (id, username, email, password, role, language, banned, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, 'en', false, $6, $6)
           ON CONFLICT (username) DO UPDATE
             SET password = EXCLUDED.password,
                 email = COALESCE(EXCLUDED.email, "User".email),
                 role = EXCLUDED.role,
                 banned = false,
                 "updatedAt" = EXCLUDED."updatedAt""#,
    )
    .bind(id)
    .bind(username)
    .bind(email)
    .bind(pw_hash)
    .bind(UserRole::SuperAdmin)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(true)
}

/// Registers a node row from CLI (`mypanel3 node create`), e.g. for all-in-one `--mode both` installs.
/// Returns `Ok(Some(node_id))` when created, or `Ok(None)` when `if_missing` is true and a matching node already exists.
pub async fn create_local_node(
    pool: &PgPool,
    name: &str,
    host: &str,
    port: i32,
    api_port: i32,
    secret: &str,
    if_missing: bool,
) -> Result<Option<String>> {
    let trimmed_name = name.trim();
    let trimmed_host = host.trim();
    let trimmed_secret = secret.trim();

    anyhow::ensure!(!trimmed_name.is_empty(), "Node name cannot be empty");
    anyhow::ensure!(!trimmed_host.is_empty(), "Node host cannot be empty");
    anyhow::ensure!(
        (1..=65535).contains(&port) && (1..=65535).contains(&api_port),
        "Node ports must be within 1..=65535"
    );
    anyhow::ensure!(
        trimmed_secret.len() >= 16
            && trimmed_secret.len() <= 256
            && !trimmed_secret
                .chars()
                .any(|c| c.is_whitespace() || c.is_control()),
        "Node secret must be 16 to 256 characters without whitespace or control characters"
    );

    let secret_hash = sha256_hex(trimmed_secret);

    if if_missing {
        let existing: Option<String> = sqlx::query_scalar(
            r#"SELECT id FROM "Node"
               WHERE host = $1 OR name = $2 OR "secretHash" = $3
               ORDER BY "createdAt" ASC LIMIT 1"#,
        )
        .bind(trimmed_host)
        .bind(trimmed_name)
        .bind(&secret_hash)
        .fetch_optional(pool)
        .await?;
        if existing.is_some() {
            return Ok(None);
        }
    }

    let id = crate::db::new_id();
    let now = crate::db::now();

    sqlx::query(
        r#"INSERT INTO "Node"
           (id, name, host, port, "apiPort", secret, "secretHash", status, tags, active, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, '{}', true, $9, $9)"#,
    )
    .bind(&id)
    .bind(trimmed_name)
    .bind(trimmed_host)
    .bind(port)
    .bind(api_port)
    .bind(trimmed_secret)
    .bind(&secret_hash)
    .bind(NodeStatus::Offline)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(Some(id))
}
