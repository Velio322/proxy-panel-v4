use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use mypanel_agent::{run as run_agent, AgentConfig};
use mypanel_panel::{config::Config as PanelConfig, db, run as run_panel, seed};
use rand::RngCore;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LogFormat {
    Text,
    Json,
}

#[derive(Debug, Parser)]
#[command(
    name = "mypanel3",
    version,
    about = "ProxyPanel v4 — multi-protocol proxy management panel and node agent"
)]
struct Cli {
    /// Path to a .env file to load before reading environment variables.
    #[arg(long, global = true, value_name = "PATH")]
    env_file: Option<PathBuf>,

    /// Log output format.
    #[arg(
        long,
        global = true,
        env = "LOG_FORMAT",
        value_enum,
        default_value_t = LogFormat::Text
    )]
    log_format: LogFormat,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the master panel HTTP/WebSocket server (default).
    Serve,
    /// Run the node worker agent daemon.
    Agent,
    /// Run database migrations and exit.
    Migrate,
    /// Administrative user management commands.
    Admin {
        #[command(subcommand)]
        action: AdminCommand,
    },
    /// Node database registration commands.
    Node {
        #[command(subcommand)]
        action: NodeCommand,
    },
    /// Generate cryptographically random secrets for .env configuration.
    GenSecrets,
}

#[derive(Debug, Subcommand)]
enum AdminCommand {
    /// Create an admin user account.
    Create(AdminCreateArgs),
}

#[derive(Debug, Args)]
struct AdminCreateArgs {
    /// Admin username.
    #[arg(long, default_value = "admin")]
    username: String,

    /// Admin password (if omitted, a strong random password is generated and printed).
    #[arg(long)]
    password: Option<String>,

    /// Optional admin email address.
    #[arg(long)]
    email: Option<String>,

    /// Succeed without error if an admin user already exists.
    #[arg(long)]
    if_missing: bool,
}

#[derive(Debug, Subcommand)]
enum NodeCommand {
    /// Register a node row in the panel database.
    Create(NodeCreateArgs),
}

#[derive(Debug, Args)]
struct NodeCreateArgs {
    /// Human-readable node name.
    #[arg(long)]
    name: String,

    /// Public IP or domain of the node.
    #[arg(long)]
    host: String,

    /// Primary public proxy port.
    #[arg(long, default_value_t = 443)]
    port: i32,

    /// Worker HTTP API port.
    #[arg(long, default_value_t = 2222)]
    api_port: i32,

    /// Pre-shared RPC secret (16..=256 characters).
    #[arg(long)]
    secret: String,

    /// Succeed without error if a matching node already exists.
    #[arg(long)]
    if_missing: bool,
}

fn load_env(env_file: Option<&PathBuf>) -> Result<()> {
    match env_file {
        Some(path) => {
            dotenvy::from_path(path)
                .with_context(|| format!("failed to load env file '{}'", path.display()))?;
        }
        None => {
            let _ = dotenvy::dotenv();
        }
    }
    Ok(())
}

fn init_tracing(format: LogFormat) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    match format {
        LogFormat::Text => {
            let _ = fmt().with_env_filter(filter).try_init();
        }
        LogFormat::Json => {
            let _ = fmt().json().with_env_filter(filter).try_init();
        }
    }
}

fn random_hex(num_bytes: usize) -> String {
    let mut buf = vec![0u8; num_bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    let mut out = String::with_capacity(num_bytes * 2);
    for byte in buf {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn random_password(len: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*-_";
    let mut buf = vec![0u8; len];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    buf.into_iter()
        .map(|b| CHARSET[usize::from(b) % CHARSET.len()] as char)
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    load_env(cli.env_file.as_ref())?;
    init_tracing(cli.log_format);

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => {
            let cfg = PanelConfig::from_env().context("failed to load panel configuration")?;
            run_panel(cfg).await
        }
        Command::Agent => {
            let cfg = AgentConfig::from_env().context("failed to load agent configuration")?;
            run_agent(cfg).await
        }
        Command::Migrate => {
            let cfg = PanelConfig::from_env().context("failed to load panel configuration")?;
            let pool = db::connect_pool(&cfg.database_url, cfg.db_max_connections)
                .await
                .context("failed to connect to database")?;
            db::run_migrations(&pool)
                .await
                .context("failed to run database migrations")?;
            tracing::info!("database migrations applied successfully");
            Ok(())
        }
        Command::Admin {
            action: AdminCommand::Create(args),
        } => {
            let cfg = PanelConfig::from_env().context("failed to load panel configuration")?;
            let pool = db::connect_pool(&cfg.database_url, cfg.db_max_connections)
                .await
                .context("failed to connect to database")?;
            db::run_migrations(&pool)
                .await
                .context("failed to run database migrations")?;

            let generated = args.password.is_none();
            let password = args.password.unwrap_or_else(|| random_password(20));

            let created = seed::create_admin(
                &pool,
                &args.username,
                &password,
                args.email.as_deref(),
                args.if_missing,
            )
            .await
            .context("failed to create admin user")?;

            if created && generated {
                println!("Admin user '{}' created.", args.username);
                println!("Generated password: {password}");
            } else if created {
                println!("Admin user '{}' created.", args.username);
            } else {
                println!("Admin user already exists; skipping (--if-missing).");
            }
            Ok(())
        }
        Command::Node {
            action: NodeCommand::Create(args),
        } => {
            let cfg = PanelConfig::from_env().context("failed to load panel configuration")?;
            let pool = db::connect_pool(&cfg.database_url, cfg.db_max_connections)
                .await
                .context("failed to connect to database")?;
            db::run_migrations(&pool)
                .await
                .context("failed to run database migrations")?;

            match seed::create_local_node(
                &pool,
                &args.name,
                &args.host,
                args.port,
                args.api_port,
                &args.secret,
                args.if_missing,
            )
            .await
            .context("failed to register node")?
            {
                Some(node_id) => {
                    println!("Node '{}' registered (id: {node_id}).", args.name.trim());
                }
                None => {
                    println!("Node already exists; skipping (--if-missing).");
                }
            }
            Ok(())
        }
        Command::GenSecrets => {
            println!("JWT_SECRET={}", random_hex(32));
            println!("ENCRYPTION_KEY={}", random_hex(32));
            println!("METRICS_TOKEN={}", random_hex(24));
            Ok(())
        }
    }
}
