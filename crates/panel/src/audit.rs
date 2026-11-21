use serde_json::{json, Value};
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub user_id: Option<String>,
    pub action: String,
    pub resource: String,
    pub resource_id: Option<String>,
    pub details: Option<Value>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

impl AuditEntry {
    pub fn new(action: impl Into<String>, resource: impl Into<String>) -> Self {
        Self {
            user_id: None,
            action: action.into(),
            resource: resource.into(),
            resource_id: None,
            details: None,
            ip: None,
            user_agent: None,
        }
    }

    pub fn user(mut self, user_id: impl Into<String>) -> Self {
        let s = user_id.into();
        if !s.is_empty() {
            self.user_id = Some(s);
        }
        self
    }

    pub fn resource_id(mut self, id: impl Into<String>) -> Self {
        let s = id.into();
        if !s.is_empty() {
            self.resource_id = Some(s);
        }
        self
    }

    pub fn details(mut self, details: Value) -> Self {
        self.details = Some(sanitize_value(&details));
        self
    }

    pub fn ip(mut self, ip: impl Into<String>) -> Self {
        let s = ip.into();
        if !s.is_empty() {
            self.ip = Some(s);
        }
        self
    }

    pub fn user_agent(mut self, ua: Option<String>) -> Self {
        self.user_agent = ua.filter(|s| !s.is_empty());
        self
    }
}

/// Recursively redacts any object keys containing `password`, `secret`, `token`, `key`, or `credential`.
pub fn sanitize_value(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, val) in map {
                let lower = k.to_ascii_lowercase();
                if lower.contains("password")
                    || lower.contains("secret")
                    || lower.contains("token")
                    || lower.contains("key")
                    || lower.contains("credential")
                {
                    out.insert(k.clone(), json!("[REDACTED]"));
                } else {
                    out.insert(k.clone(), sanitize_value(val));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sanitize_value).collect()),
        other => other.clone(),
    }
}

pub async fn write_audit(pool: &PgPool, entry: AuditEntry) {
    let id = crate::db::new_id();
    let now = crate::db::now();
    let details = entry.details.map(|v| sanitize_value(&v));
    if let Err(e) = sqlx::query(
        r#"INSERT INTO "AuditLog" (id, "userId", action, resource, "resourceId", details, ip, "userAgent", "createdAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
    )
    .bind(id)
    .bind(entry.user_id)
    .bind(entry.action)
    .bind(entry.resource)
    .bind(entry.resource_id)
    .bind(details)
    .bind(entry.ip)
    .bind(entry.user_agent)
    .bind(now)
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "[AuditLog] Failed to create audit log");
    }
}

pub fn spawn(pool: &PgPool, entry: AuditEntry) {
    let db = pool.clone();
    tokio::spawn(async move {
        write_audit(&db, entry).await;
    });
}
