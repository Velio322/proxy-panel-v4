use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

pub use mypanel_proto::Protocol;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = r#""Protocol""#, rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DbProtocol {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
    Hysteria2,
    Naiveproxy,
    Mieru,
    Tuic,
    Anytls,
    Shadowtls,
}

impl From<DbProtocol> for Protocol {
    fn from(p: DbProtocol) -> Self {
        match p {
            DbProtocol::Vless => Self::Vless,
            DbProtocol::Vmess => Self::Vmess,
            DbProtocol::Trojan => Self::Trojan,
            DbProtocol::Shadowsocks => Self::Shadowsocks,
            DbProtocol::Hysteria2 => Self::Hysteria2,
            DbProtocol::Naiveproxy => Self::Naiveproxy,
            DbProtocol::Mieru => Self::Mieru,
            DbProtocol::Tuic => Self::Tuic,
            DbProtocol::Anytls => Self::Anytls,
            DbProtocol::Shadowtls => Self::Shadowtls,
        }
    }
}

impl From<Protocol> for DbProtocol {
    fn from(p: Protocol) -> Self {
        match p {
            Protocol::Vless => Self::Vless,
            Protocol::Vmess => Self::Vmess,
            Protocol::Trojan => Self::Trojan,
            Protocol::Shadowsocks => Self::Shadowsocks,
            Protocol::Hysteria2 => Self::Hysteria2,
            Protocol::Naiveproxy => Self::Naiveproxy,
            Protocol::Mieru => Self::Mieru,
            Protocol::Tuic => Self::Tuic,
            Protocol::Anytls => Self::Anytls,
            Protocol::Shadowtls => Self::Shadowtls,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = r#""UserRole""#, rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UserRole {
    SuperAdmin,
    Admin,
    Reseller,
    Operator,
}

impl UserRole {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::SuperAdmin => "SUPER_ADMIN",
            Self::Admin => "ADMIN",
            Self::Reseller => "RESELLER",
            Self::Operator => "OPERATOR",
        }
    }

    pub const fn rank(&self) -> u8 {
        match self {
            Self::SuperAdmin => 4,
            Self::Admin => 3,
            Self::Reseller => 2,
            Self::Operator => 1,
        }
    }

    pub const fn can_manage(self, target: Self) -> bool {
        self.rank() > target.rank()
    }
}

impl fmt::Display for UserRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for UserRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "SUPER_ADMIN" => Ok(Self::SuperAdmin),
            "ADMIN" => Ok(Self::Admin),
            "RESELLER" => Ok(Self::Reseller),
            "OPERATOR" => Ok(Self::Operator),
            _ => Err(format!("Invalid user role: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = r#""PlanType""#, rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanType {
    User,
    Reseller,
}

impl PlanType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::User => "USER",
            Self::Reseller => "RESELLER",
        }
    }
}

impl FromStr for PlanType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "USER" => Ok(Self::User),
            "RESELLER" => Ok(Self::Reseller),
            _ => Err(format!("Invalid plan type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = r#""SubFormat""#, rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DbSubFormat {
    Base64,
    Json,
    Clash,
    Singbox,
    Original,
}

impl DbSubFormat {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Base64 => "BASE64",
            Self::Json => "JSON",
            Self::Clash => "CLASH",
            Self::Singbox => "SINGBOX",
            Self::Original => "ORIGINAL",
        }
    }
}

impl FromStr for DbSubFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "BASE64" => Ok(Self::Base64),
            "JSON" => Ok(Self::Json),
            "CLASH" => Ok(Self::Clash),
            "SINGBOX" => Ok(Self::Singbox),
            "ORIGINAL" => Ok(Self::Original),
            _ => Err(format!("Invalid SubFormat: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = r#""NodeStatus""#, rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeStatus {
    Online,
    Offline,
    Error,
    Maintenance,
}

impl NodeStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Online => "ONLINE",
            Self::Offline => "OFFLINE",
            Self::Error => "ERROR",
            Self::Maintenance => "MAINTENANCE",
        }
    }
}

impl FromStr for NodeStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "ONLINE" => Ok(Self::Online),
            "OFFLINE" => Ok(Self::Offline),
            "ERROR" => Ok(Self::Error),
            "MAINTENANCE" => Ok(Self::Maintenance),
            _ => Err(format!("Invalid NodeStatus: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = r#""NotificationType""#, rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NotificationType {
    Info,
    Warning,
    Error,
    Success,
    TrafficLimit,
    SubscriptionExpired,
    NodeOffline,
    PaymentReceived,
}

impl NotificationType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
            Self::Success => "SUCCESS",
            Self::TrafficLimit => "TRAFFIC_LIMIT",
            Self::SubscriptionExpired => "SUBSCRIPTION_EXPIRED",
            Self::NodeOffline => "NODE_OFFLINE",
            Self::PaymentReceived => "PAYMENT_RECEIVED",
        }
    }
}

impl FromStr for NotificationType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "INFO" => Ok(Self::Info),
            "WARNING" => Ok(Self::Warning),
            "ERROR" => Ok(Self::Error),
            "SUCCESS" => Ok(Self::Success),
            "TRAFFIC_LIMIT" => Ok(Self::TrafficLimit),
            "SUBSCRIPTION_EXPIRED" => Ok(Self::SubscriptionExpired),
            "NODE_OFFLINE" => Ok(Self::NodeOffline),
            "PAYMENT_RECEIVED" => Ok(Self::PaymentReceived),
            _ => Err(format!("Invalid NotificationType: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = r#""BackupType""#, rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BackupType {
    Database,
    Config,
    Full,
}

impl BackupType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Database => "DATABASE",
            Self::Config => "CONFIG",
            Self::Full => "FULL",
        }
    }
}

impl FromStr for BackupType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "DATABASE" => Ok(Self::Database),
            "CONFIG" => Ok(Self::Config),
            "FULL" => Ok(Self::Full),
            _ => Err(format!("Invalid BackupType: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = r#""BackupStatus""#, rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BackupStatus {
    Success,
    Failed,
    InProgress,
}

impl BackupStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Failed => "FAILED",
            Self::InProgress => "IN_PROGRESS",
        }
    }
}

impl FromStr for BackupStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "SUCCESS" | "COMPLETED" => Ok(Self::Success),
            "FAILED" => Ok(Self::Failed),
            "IN_PROGRESS" | "PENDING" => Ok(Self::InProgress),
            _ => Err(format!("Invalid BackupStatus: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrafficResetStrategy {
    NoReset,
    Day,
    Week,
    Month,
    MonthRolling,
    OnHold,
}

impl Default for TrafficResetStrategy {
    fn default() -> Self {
        Self::NoReset
    }
}

impl TrafficResetStrategy {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NoReset => "NO_RESET",
            Self::Day => "DAY",
            Self::Week => "WEEK",
            Self::Month => "MONTH",
            Self::MonthRolling => "MONTH_ROLLING",
            Self::OnHold => "ON_HOLD",
        }
    }
}

impl FromStr for TrafficResetStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "NO_RESET" | "NONE" | "" => Ok(Self::NoReset),
            "DAY" | "DAILY" => Ok(Self::Day),
            "WEEK" | "WEEKLY" => Ok(Self::Week),
            "MONTH" | "MONTHLY" => Ok(Self::Month),
            "MONTH_ROLLING" | "ROLLING" => Ok(Self::MonthRolling),
            "ON_HOLD" | "HOLD" => Ok(Self::OnHold),
            _ => Err(format!("Invalid TrafficResetStrategy: {s}")),
        }
    }
}
