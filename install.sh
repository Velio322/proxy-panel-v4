#!/usr/bin/env bash
set -euo pipefail

# mypanel3 (ProxyPanel v4) idempotent installer for Debian 11+ / Ubuntu 20.04+ (amd64 / arm64)
# Supports 4 scenarios:
#   1) Panel + Node all-in-one (--mode both) with HAProxy SNI-mux on :443
#   2) Panel only (--mode panel)
#   3) Node only (--mode node) — leaves :443 free for VLESS Reality by default
#   4) Connecting a new node to an existing panel (auto-downloads mypanel3 binary & pairs via WSS)

BIN_DEST="/usr/local/bin/mypanel3"
ETC_DIR="/etc/mypanel3"
PANEL_ENV="${ETC_DIR}/mypanel3.env"
NODE_ENV_FILE="${ETC_DIR}/node.env"
ADMIN_CREDS_FILE="${ETC_DIR}/admin-credentials.txt"
PANEL_DATA_DIR="/var/lib/mypanel3"
NODE_DATA_DIR="/var/lib/mypanel3-node"
NODE_TLS_DIR="/etc/mypanel3-node/tls"
CADDY_FILE="/etc/caddy/Caddyfile"
CADDY_CERTS_DIR="/etc/caddy/certs"
HAPROXY_FILE="/etc/haproxy/haproxy.cfg"
SYSTEMD_DIR="/etc/systemd/system"

MODE=""
DOMAIN=""
SUB_DOMAIN=""
EMAIL=""
PROXY_MODE="caddy"
SNI_MUX="auto"
TLS_MODE="auto"
TLS_MODE_EXPLICIT="false"
TLS_CERTS_DOMAIN=""
ADMIN_USER="admin"
ADMIN_PASS=""
GEN_ADMIN_PASS="false"
MASTER_URL=""
NODE_DOMAIN=""
NODE_NAME=""
NODE_SECRET=""
API_PORT="2087"
CORES_SPEC="all"
ENABLE_BBR="auto"
NON_INTERACTIVE="false"
UPGRADE_ONLY="false"
BINARY_PATH=""
BINARY_URL=""
ANSWERS_FILE=""
GITHUB_REPO="${GITHUB_REPO:-Velio322/proxy-panel-v4}"

ARCH=""
PUBLIC_IP=""
AGENT_WORKER_PORT="2087"
AGENT_WORKER_HOST="0.0.0.0"

log() {
    printf '[mypanel3] %s\n' "$*"
}

warn() {
    printf '[mypanel3] WARNING: %s\n' "$*" >&2
}

die() {
    printf '[mypanel3] ERROR: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Usage: sudo ./install.sh [OPTIONS]

Topologies:
  --mode <both|panel|node>      Installation topology:
                                  both  - Panel + Node on one server (HAProxy SNI mux on :443)
                                  panel - Control panel + PostgreSQL + Caddy only
                                  node  - Node worker agent + proxy cores only (:443 kept free)
  --upgrade                     Upgrade binary, run migrations, and restart services
  --binary <PATH>               Path to prebuilt mypanel3 binary
  --binary-url <URL>            HTTP(S) URL to download mypanel3 binary

Panel options:
  --domain <DOMAIN>             Panel public domain (e.g. panel.example.com)
  --sub-domain <DOMAIN>         Optional separate subscription domain (e.g. sub.example.com)
  --email, --admin-email <ADDR> ACME / admin email address
  --proxy <caddy|none>          Reverse proxy for panel (default: caddy)
  --sni-mux / --no-sni-mux      In --mode both, share :443 between Panel HTTPS & Xray Reality via HAProxy
                                (default: enabled when --domain is set and --proxy=caddy)
  --admin-user <USER>           Initial admin username (default: admin)
  --admin-pass <PASS>           Initial admin password (min 12 chars)
  --gen-admin-pass              Generate a random admin password

Node options:
  --master-url <URL>            Master panel URL (for --mode node, e.g. https://panel.example.com)
  --node-domain <DOMAIN|IP>     Node public domain or IPv4/IPv6 (auto-detected if omitted)
  --node-name <NAME>            Human-readable node name (default: hostname)
  --node-secret <SECRET>        Node RPC secret from Panel (Nodes -> Add node)
  --api-port <PORT>             Worker API port (default: 2087)
  --tls-mode <none|port2087|auto|internal>
                                Node Worker-API transport (default for node: none on :2087, :443 free;
                                'port2087' uses Caddy HTTPS on :2087 -> loopback; 'auto'/'internal' uses :443)
  --tls-certs-domain <DOMAIN>   Issue Let's Encrypt certbot files into /etc/mypanel3-node/tls
  --cores <all|list|none>       Comma-separated cores: xray,singbox,naive,mieru (default: all)
  --skip-cores                  Alias for --cores none
  --bbr / --no-bbr              Configure fq + BBR sysctl tuning (default: enabled for node/both)

Automation:
  --answers <FILE|->            Read wizard answers line-by-line from file or stdin
  -y, --yes, --non-interactive  Non-interactive mode (use flags/defaults, auto-gen passwords)
  -h, --help                    Show this help message
EOF
}

normalize_master_url() {
    local u="$1"
    u="${u%/}"
    u="${u%/ws/worker}"
    if [[ "$u" == wss://* ]]; then
        u="https://${u#wss://}"
    elif [[ "$u" == ws://* ]]; then
        u="http://${u#ws://}"
    fi
    printf '%s' "$u"
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --mode)
                [[ $# -ge 2 ]] || die "--mode requires a value"
                MODE="$2"
                shift 2
                ;;
            --domain)
                [[ $# -ge 2 ]] || die "--domain requires a value"
                DOMAIN="$2"
                shift 2
                ;;
            --sub-domain)
                [[ $# -ge 2 ]] || die "--sub-domain requires a value"
                SUB_DOMAIN="$2"
                shift 2
                ;;
            --email|--admin-email)
                [[ $# -ge 2 ]] || die "$1 requires a value"
                EMAIL="$2"
                shift 2
                ;;
            --proxy)
                [[ $# -ge 2 ]] || die "--proxy requires a value"
                PROXY_MODE="$2"
                shift 2
                ;;
            --sni-mux)
                SNI_MUX="true"
                shift
                ;;
            --no-sni-mux)
                SNI_MUX="false"
                shift
                ;;
            --tls-mode)
                [[ $# -ge 2 ]] || die "--tls-mode requires a value"
                TLS_MODE="$2"
                TLS_MODE_EXPLICIT="true"
                shift 2
                ;;
            --tls-certs-domain)
                [[ $# -ge 2 ]] || die "--tls-certs-domain requires a value"
                TLS_CERTS_DOMAIN="$2"
                shift 2
                ;;
            --admin-user)
                [[ $# -ge 2 ]] || die "--admin-user requires a value"
                ADMIN_USER="$2"
                shift 2
                ;;
            --admin-pass)
                [[ $# -ge 2 ]] || die "--admin-pass requires a value"
                ADMIN_PASS="$2"
                shift 2
                ;;
            --gen-admin-pass)
                GEN_ADMIN_PASS="true"
                shift
                ;;
            --master-url|--panel-url)
                [[ $# -ge 2 ]] || die "$1 requires a value"
                MASTER_URL="$(normalize_master_url "$2")"
                shift 2
                ;;
            --node-domain|--node-host)
                [[ $# -ge 2 ]] || die "$1 requires a value"
                NODE_DOMAIN="$2"
                shift 2
                ;;
            --node-name)
                [[ $# -ge 2 ]] || die "--node-name requires a value"
                NODE_NAME="$2"
                shift 2
                ;;
            --node-secret)
                [[ $# -ge 2 ]] || die "--node-secret requires a value"
                NODE_SECRET="$2"
                shift 2
                ;;
            --node-id)
                [[ $# -ge 2 ]] || die "--node-id requires a value"
                shift 2
                ;;
            --api-port)
                [[ $# -ge 2 ]] || die "--api-port requires a value"
                API_PORT="$2"
                shift 2
                ;;
            --cores)
                [[ $# -ge 2 ]] || die "--cores requires a value"
                CORES_SPEC="$2"
                shift 2
                ;;
            --skip-cores)
                CORES_SPEC="none"
                shift
                ;;
            --bbr)
                ENABLE_BBR="true"
                shift
                ;;
            --no-bbr)
                ENABLE_BBR="false"
                shift
                ;;
            --answers)
                [[ $# -ge 2 ]] || die "--answers requires a value"
                ANSWERS_FILE="$2"
                shift 2
                ;;
            --upgrade)
                UPGRADE_ONLY="true"
                shift
                ;;
            --binary)
                [[ $# -ge 2 ]] || die "--binary requires a value"
                BINARY_PATH="$2"
                shift 2
                ;;
            --binary-url)
                [[ $# -ge 2 ]] || die "--binary-url requires a value"
                BINARY_URL="$2"
                shift 2
                ;;
            -y|--yes|--non-interactive)
                NON_INTERACTIVE="true"
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                die "Unknown option: $1 (use --help for usage)"
                ;;
        esac
    done
}

ANSWERS_FD=""
init_answers_fd() {
    if [[ -n "$ANSWERS_FILE" ]]; then
        if [[ "$ANSWERS_FILE" == "-" ]]; then
            exec 3<&0
        else
            [[ -f "$ANSWERS_FILE" ]] || die "Answers file not found: $ANSWERS_FILE"
            exec 3<"$ANSWERS_FILE"
        fi
        ANSWERS_FD="3"
    fi
}

prompt_line() {
    local prompt_text="$1"
    local default_val="${2:-}"
    local result=""

    if [[ -n "$ANSWERS_FD" ]]; then
        IFS= read -r -u "$ANSWERS_FD" result || result="$default_val"
        printf '%s' "${result:-$default_val}"
        return
    fi

    if [[ "$NON_INTERACTIVE" == "true" ]]; then
        printf '%s' "$default_val"
        return
    fi

    if [[ -n "$default_val" ]]; then
        printf '%s [%s]: ' "$prompt_text" "$default_val" >&2
    else
        printf '%s: ' "$prompt_text" >&2
    fi
    if [[ ! -t 0 ]] && [[ -r /dev/tty ]]; then
        IFS= read -r result </dev/tty || result=""
    else
        IFS= read -r result || result=""
    fi
    printf '%s' "${result:-$default_val}"
}

prompt_secret() {
    local prompt_text="$1"
    local result=""

    if [[ -n "$ANSWERS_FD" ]]; then
        IFS= read -r -u "$ANSWERS_FD" result || result=""
        printf '%s' "$result"
        return
    fi

    if [[ "$NON_INTERACTIVE" == "true" ]]; then
        printf '%s' ""
        return
    fi

    printf '%s: ' "$prompt_text" >&2
    if [[ ! -t 0 ]] && [[ -r /dev/tty ]]; then
        IFS= read -r -s result </dev/tty || result=""
    else
        IFS= read -r -s result || result=""
    fi
    printf '\n' >&2
    printf '%s' "$result"
}

check_root() {
    if [[ "$(id -u)" -ne 0 ]]; then
        die "Please run as root (e.g. sudo ./install.sh)"
    fi
}

check_os_and_arch() {
    [[ -f /etc/os-release ]] || die "Cannot detect OS (/etc/os-release missing)"
    local os_id=""
    local os_ver=""
    os_id="$(sed -nE 's/^ID="?([^"]+)"?/\1/p' /etc/os-release | head -n 1)"
    os_ver="$(sed -nE 's/^VERSION_ID="?([^"]+)"?/\1/p' /etc/os-release | head -n 1)"
    os_ver="${os_ver:-0}"

    case "$os_id" in
        debian)
            if [[ "${os_ver%%.*}" -lt 11 ]]; then
                die "Debian 11+ is required (found $os_ver)"
            fi
            ;;
        ubuntu)
            if [[ "${os_ver%%.*}" -lt 20 ]]; then
                die "Ubuntu 20.04+ is required (found $os_ver)"
            fi
            ;;
        *)
            warn "Untested OS '$os_id'; continuing anyway"
            ;;
    esac

    local arch=""
    arch="$(uname -m)"
    case "$arch" in
        x86_64|amd64) ARCH="amd64" ;;
        aarch64|arm64) ARCH="arm64" ;;
        *) die "Unsupported architecture: $arch (supported: amd64, arm64)" ;;
    esac

    if ! command -v systemctl >/dev/null 2>&1; then
        die "systemd is required (systemctl not found)"
    fi
}

detect_public_ipv4() {
    if [[ -n "$PUBLIC_IP" ]]; then
        printf '%s' "$PUBLIC_IP"
        return
    fi
    local ip=""
    ip="$(curl -4fsS --max-time 4 https://api.ipify.org 2>/dev/null || curl -4fsS --max-time 4 https://ifconfig.me 2>/dev/null || true)"
    if [[ ! "$ip" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]]; then
        ip="$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="src") {print $(i+1); exit}}' || true)"
    fi
    if [[ ! "$ip" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]]; then
        ip="$(hostname -I 2>/dev/null | awk '{print $1}' || true)"
    fi
    PUBLIC_IP="$ip"
    printf '%s' "$PUBLIC_IP"
}

is_ip_addr() {
    local v="$1"
    if [[ "$v" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]]; then
        return 0
    fi
    if [[ "$v" == *:* ]]; then
        return 0
    fi
    return 1
}

validate_domain() {
    local d="$1"
    [[ "$d" =~ ^[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?(\.[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?)+$ ]]
}

ensure_swap() {
    local mem_kb=0
    local swap_kb=0
    mem_kb="$(awk '/MemTotal/ {print $2}' /proc/meminfo)"
    swap_kb="$(awk '/SwapTotal/ {print $2}' /proc/meminfo)"
    local total_mb=$(( (mem_kb + swap_kb) / 1024 ))

    if [[ "$total_mb" -lt 1900 ]] && [[ ! -f /swapfile ]]; then
        log "Total RAM+swap is ${total_mb}MB (<2G); creating 2G /swapfile..."
        if fallocate -l 2G /swapfile 2>/dev/null || dd if=/dev/zero of=/swapfile bs=1M count=2048 status=none; then
            chmod 0600 /swapfile
            if mkswap /swapfile >/dev/null 2>&1 && swapon /swapfile 2>/dev/null; then
                if ! grep -q '^/swapfile ' /etc/fstab; then
                    printf '/swapfile none swap sw 0 0\n' >> /etc/fstab
                fi
            else
                warn "Could not activate /swapfile (container environment?); continuing"
                rm -f /swapfile
            fi
        fi
    fi
}

configure_bbr() {
    if [[ "$ENABLE_BBR" != "true" ]]; then
        return 0
    fi
    log "Applying TCP BBR + fq sysctl tuning (/etc/sysctl.d/99-mypanel3.conf)..."
    cat > /etc/sysctl.d/99-mypanel3.conf <<'EOF'
net.core.default_qdisc = fq
net.ipv4.tcp_congestion_control = bbr
net.core.rmem_max = 16777216
net.core.wmem_max = 16777216
EOF
    sysctl --system >/dev/null 2>&1 || warn "sysctl --system reported warnings (normal in some containers)"
}

check_port_free() {
    local port="$1"
    local allowed_proc="${2:-}"
    if ! command -v ss >/dev/null 2>&1; then
        return 0
    fi
    local listeners=""
    listeners="$(ss -H -ltn "sport = :${port}" 2>/dev/null || true)"
    if [[ -n "$listeners" ]]; then
        if [[ -n "$allowed_proc" ]] && printf '%s' "$listeners" | grep -q "$allowed_proc"; then
            return 0
        fi
        if systemctl is-active --quiet mypanel3 2>/dev/null && [[ "$port" == "3000" ]]; then
            return 0
        fi
        if systemctl is-active --quiet mypanel3-agent 2>/dev/null && { [[ "$port" == "$API_PORT" ]] || [[ "$port" == "$AGENT_WORKER_PORT" ]]; }; then
            return 0
        fi
        if systemctl is-active --quiet postgresql 2>/dev/null && [[ "$port" == "5432" ]]; then
            return 0
        fi
        if systemctl is-active --quiet caddy 2>/dev/null && { [[ "$port" == "80" ]] || [[ "$port" == "443" ]] || [[ "$port" == "9443" ]] || [[ "$port" == "$API_PORT" ]]; }; then
            return 0
        fi
        if systemctl is-active --quiet haproxy 2>/dev/null && [[ "$port" == "443" ]]; then
            return 0
        fi
        die "Port ${port}/tcp is already in use. Stop the conflicting service and re-run."
    fi
}

check_dns_record() {
    local fqdn="$1"
    [[ -n "$fqdn" ]] || return 0
    if is_ip_addr "$fqdn"; then
        return 0
    fi
    if ! command -v dig >/dev/null 2>&1; then
        return 0
    fi
    local resolved=""
    resolved="$(dig +short A "$fqdn" | head -n 1 || true)"
    if [[ -z "$resolved" ]]; then
        warn "DNS A record for '$fqdn' did not resolve; TLS issuance may fail."
        return 0
    fi
    local local_ips=""
    local_ips="$(hostname -I 2>/dev/null || true)"
    if ! printf '%s\n' "$local_ips" | tr ' ' '\n' | grep -qx "$resolved"; then
        warn "DNS A record for '$fqdn' ($resolved) does not match local IPs ($local_ips)."
    fi
}

download_binary_from_panel_or_url() {
    local url="$1"
    local secret_hdr="${2:-}"
    local tmp_dir=""
    tmp_dir="$(mktemp -d)"
    local bin_tmp="${tmp_dir}/mypanel3"
    local hdr_tmp="${tmp_dir}/headers.txt"

    log "Downloading mypanel3 binary (${ARCH})..."
    local curl_args=(-fsSL -D "$hdr_tmp" -o "$bin_tmp")
    if [[ -n "$secret_hdr" ]]; then
        curl_args+=(-H "X-Node-Secret: ${secret_hdr}")
    fi

    if ! curl "${curl_args[@]}" "$url"; then
        rm -rf "$tmp_dir"
        die "Failed to download mypanel3 binary from $url"
    fi

    [[ -s "$bin_tmp" ]] || {
        rm -rf "$tmp_dir"
        die "Downloaded mypanel3 binary is empty"
    }

    local expected_sha=""
    expected_sha="$(grep -i '^x-binary-sha256:' "$hdr_tmp" 2>/dev/null | head -n 1 | awk '{print $2}' | tr -d '\r\n' || true)"
    if [[ -n "$expected_sha" ]]; then
        local actual_sha=""
        actual_sha="$(sha256sum "$bin_tmp" | awk '{print $1}')"
        if [[ "$actual_sha" != "$expected_sha" ]]; then
            rm -rf "$tmp_dir"
            die "SHA-256 verification failed for mypanel3 binary (expected $expected_sha, got $actual_sha)"
        fi
        log "Binary SHA-256 verified (${expected_sha:0:16}...)."
    fi

    install -m 0755 "$bin_tmp" "$BIN_DEST"
    rm -rf "$tmp_dir"
    BINARY_PATH="$BIN_DEST"
}

download_binary_from_github_release() {
    local asset_name="mypanel3-linux-${ARCH}"
    local base_url="https://github.com/${GITHUB_REPO}/releases/latest/download"
    local bin_url="${base_url}/${asset_name}"
    local sums_url="${base_url}/SHA256SUMS"
    local tmp_dir=""
    tmp_dir="$(mktemp -d)"
    local bin_tmp="${tmp_dir}/mypanel3"
    local sums_tmp="${tmp_dir}/SHA256SUMS"

    log "Downloading ${asset_name} from GitHub Releases (${GITHUB_REPO})..."
    if ! curl -fsSL -o "$bin_tmp" "$bin_url" || [[ ! -s "$bin_tmp" ]]; then
        rm -rf "$tmp_dir"
        die "Failed to download ${asset_name} from ${bin_url}. Pass --binary <PATH> or --binary-url <URL>."
    fi

    if curl -fsSL -o "$sums_tmp" "$sums_url" 2>/dev/null; then
        local expected_sha=""
        expected_sha="$(grep -E "[[:space:]](\./)?${asset_name}$" "$sums_tmp" 2>/dev/null | head -n 1 | awk '{print $1}' | tr -d '\r\n' || true)"
        if [[ -n "$expected_sha" ]]; then
            local actual_sha=""
            actual_sha="$(sha256sum "$bin_tmp" | awk '{print $1}')"
            if [[ "$actual_sha" != "$expected_sha" ]]; then
                rm -rf "$tmp_dir"
                die "SHA-256 verification failed for ${asset_name} (expected ${expected_sha}, got ${actual_sha})"
            fi
            log "Release SHA-256 verified (${expected_sha:0:16}...)."
        fi
    fi

    install -m 0755 "$bin_tmp" "$BIN_DEST"
    rm -rf "$tmp_dir"
    BINARY_PATH="$BIN_DEST"
}

resolve_binary() {
    if [[ -n "$BINARY_PATH" ]]; then
        [[ -x "$BINARY_PATH" ]] || die "Binary not found or not executable: $BINARY_PATH"
        return 0
    fi

    if [[ -n "$BINARY_URL" ]]; then
        download_binary_from_panel_or_url "$BINARY_URL" "$NODE_SECRET"
        return 0
    fi

    local script_src="${BASH_SOURCE[0]:-}"
    local script_dir=""
    if [[ -n "$script_src" ]] && [[ -e "$script_src" ]]; then
        script_dir="$(cd "$(dirname "$script_src")" && pwd 2>/dev/null || true)"
    fi
    if [[ -n "$script_dir" ]] && [[ -x "${script_dir}/mypanel3" ]]; then
        BINARY_PATH="${script_dir}/mypanel3"
        return 0
    elif [[ -n "$script_dir" ]] && [[ -x "${script_dir}/target/release/mypanel3" ]]; then
        BINARY_PATH="${script_dir}/target/release/mypanel3"
        return 0
    fi

    if [[ "$MODE" == "node" ]] && [[ -n "$MASTER_URL" ]] && [[ -n "$NODE_SECRET" ]]; then
        local auto_url="${MASTER_URL%/}/api/v1/nodes/self/binary?arch=${ARCH}"
        download_binary_from_panel_or_url "$auto_url" "$NODE_SECRET"
        return 0
    fi

    if [[ "$UPGRADE_ONLY" != "true" ]] && [[ -x "$BIN_DEST" ]]; then
        BINARY_PATH="$BIN_DEST"
        return 0
    fi

    download_binary_from_github_release
}

install_binary() {
    resolve_binary
    if [[ "$BINARY_PATH" != "$BIN_DEST" ]]; then
        log "Installing binary from $BINARY_PATH to $BIN_DEST"
        install -m 0755 "$BINARY_PATH" "$BIN_DEST"
    fi
}

install_base_packages() {
    log "Installing base packages..."
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends \
        ca-certificates curl gnupg lsb-release openssl unzip xz-utils tar dnsutils iproute2 jq
}

install_postgres() {
    if ! command -v psql >/dev/null 2>&1; then
        log "Installing PostgreSQL..."
        export DEBIAN_FRONTEND=noninteractive
        apt-get install -y -qq --no-install-recommends postgresql postgresql-client
    fi
    systemctl enable --now postgresql
}

install_caddy() {
    if ! command -v caddy >/dev/null 2>&1; then
        log "Installing Caddy..."
        apt-get install -y -qq --no-install-recommends debian-keyring debian-archive-keyring apt-transport-https
        local keyring="/usr/share/keyrings/caddy-stable-archive-keyring.gpg"
        curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
            | gpg --dearmor --yes -o "$keyring"
        chmod 0644 "$keyring"
        printf 'deb [signed-by=%s] https://dl.cloudsmith.io/public/caddy/stable/deb/debian any-version main\n' "$keyring" \
            > /etc/apt/sources.list.d/caddy-stable.list
        chmod 0644 /etc/apt/sources.list.d/caddy-stable.list
        apt-get update -qq
        apt-get install -y -qq caddy
    fi
}

install_haproxy() {
    if ! command -v haproxy >/dev/null 2>&1; then
        log "Installing HAProxy for :443 SNI multiplexing..."
        export DEBIAN_FRONTEND=noninteractive
        apt-get install -y -qq --no-install-recommends haproxy
    fi
}

should_install_core() {
    local core="$1"
    case "$CORES_SPEC" in
        all) return 0 ;;
        none|"") return 1 ;;
        *) [[ ",${CORES_SPEC}," == *",${core},"* ]] ;;
    esac
}

install_proxy_cores() {
    mkdir -p /usr/local/bin /usr/local/share/xray "$NODE_DATA_DIR"

    if should_install_core "xray" && [[ ! -x /usr/local/bin/xray ]]; then
        log "Installing Xray-core..."
        local xray_arch="64"
        [[ "$ARCH" == "arm64" ]] && xray_arch="arm64-v8a"
        local tmp_dir=""
        tmp_dir="$(mktemp -d)"
        if curl -fsSL "https://github.com/XTLS/Xray-core/releases/latest/download/Xray-linux-${xray_arch}.zip" -o "${tmp_dir}/xray.zip"; then
            unzip -q "${tmp_dir}/xray.zip" -d "${tmp_dir}/xray"
            install -m 0755 "${tmp_dir}/xray/xray" /usr/local/bin/xray
            [[ -f "${tmp_dir}/xray/geoip.dat" ]] && install -m 0644 "${tmp_dir}/xray/geoip.dat" /usr/local/share/xray/geoip.dat
            [[ -f "${tmp_dir}/xray/geosite.dat" ]] && install -m 0644 "${tmp_dir}/xray/geosite.dat" /usr/local/share/xray/geosite.dat
        else
            warn "Failed to download Xray-core; install /usr/local/bin/xray manually if needed."
        fi
        rm -rf "$tmp_dir"
    fi

    if should_install_core "singbox" && [[ ! -x /usr/local/bin/sing-box ]]; then
        log "Installing sing-box..."
        local sb_ver=""
        sb_ver="$(curl -fsSL https://api.github.com/repos/SagerNet/sing-box/releases/latest | grep -oE '"tag_name":\s*"v[^"]+"' | head -n 1 | sed -E 's/.*"v([^"]+)".*/\1/' || true)"
        if [[ -n "$sb_ver" ]]; then
            local tmp_dir=""
            tmp_dir="$(mktemp -d)"
            local sb_tar="sing-box-${sb_ver}-linux-${ARCH}.tar.gz"
            if curl -fsSL "https://github.com/SagerNet/sing-box/releases/download/v${sb_ver}/${sb_tar}" -o "${tmp_dir}/sb.tar.gz"; then
                tar -xzf "${tmp_dir}/sb.tar.gz" -C "$tmp_dir"
                install -m 0755 "${tmp_dir}/sing-box-${sb_ver}-linux-${ARCH}/sing-box" /usr/local/bin/sing-box
            else
                warn "Failed to download sing-box; install /usr/local/bin/sing-box manually if needed."
            fi
            rm -rf "$tmp_dir"
        else
            warn "Could not resolve latest sing-box release; skipping."
        fi
    fi

    if should_install_core "naive" && [[ ! -x /usr/local/bin/caddy-naive ]]; then
        if [[ "$ARCH" != "amd64" ]]; then
            warn "NaiveProxy upstream does not publish prebuilt linux/${ARCH} binaries; skipping caddy-naive."
        else
            log "Installing NaiveProxy (caddy-forwardproxy)..."
            local naive_url=""
            naive_url="$(curl -fsSL https://api.github.com/repos/klzgrad/forwardproxy/releases/latest | grep -oE '"browser_download_url":\s*"[^"]+caddy[^"]*\.tar\.(xz|gz)"' | head -n 1 | sed -E 's/.*"(https:[^"]+)".*/\1/' || true)"
            if [[ -n "$naive_url" ]]; then
                local tmp_dir=""
                tmp_dir="$(mktemp -d)"
                if curl -fsSL "$naive_url" -o "${tmp_dir}/naive.tar.xz"; then
                    tar -xf "${tmp_dir}/naive.tar.xz" -C "$tmp_dir"
                    local naive_bin=""
                    naive_bin="$(find "$tmp_dir" -type f -name caddy | head -n 1 || true)"
                    if [[ -n "$naive_bin" ]]; then
                        install -m 0755 "$naive_bin" /usr/local/bin/caddy-naive
                    else
                        warn "Could not locate caddy binary inside NaiveProxy archive."
                    fi
                else
                    warn "Failed to download NaiveProxy archive."
                fi
                rm -rf "$tmp_dir"
            else
                warn "Could not resolve latest NaiveProxy release URL; skipping."
            fi
        fi
    fi

    if should_install_core "mieru" && [[ ! -x /usr/local/bin/mita ]]; then
        log "Installing Mieru server (mita)..."
        local mita_url=""
        mita_url="$(curl -fsSL https://api.github.com/repos/enfein/mieru/releases/latest | grep -oE "\"browser_download_url\":\s*\"https://[^\"]*mita_[^\"]*_linux_${ARCH}\\.tar\\.gz\"" | head -n 1 | sed -E 's/.*"(https:[^"]+)".*/\1/' || true)"
        if [[ -n "$mita_url" ]]; then
            local tmp_dir=""
            tmp_dir="$(mktemp -d)"
            if curl -fsSL "$mita_url" -o "${tmp_dir}/mita.tar.gz"; then
                tar -xzf "${tmp_dir}/mita.tar.gz" -C "$tmp_dir"
                local mita_bin=""
                mita_bin="$(find "$tmp_dir" -type f -name mita | head -n 1 || true)"
                if [[ -n "$mita_bin" ]]; then
                    install -m 0755 "$mita_bin" /usr/local/bin/mita
                else
                    warn "Could not locate mita binary inside Mieru archive."
                fi
            else
                warn "Failed to download Mieru archive."
            fi
            rm -rf "$tmp_dir"
        else
            warn "Could not resolve latest Mieru release URL; skipping."
        fi
    fi
}

read_existing_env() {
    local file="$1"
    local key="$2"
    if [[ -f "$file" ]]; then
        grep -E "^${key}=" "$file" | head -n 1 | cut -d'=' -f2- | sed -e 's/^"//' -e 's/"$//' || true
    fi
}

ensure_panel_user_and_dirs() {
    if ! id -u mypanel3 >/dev/null 2>&1; then
        useradd --system --home-dir "$PANEL_DATA_DIR" --shell /usr/sbin/nologin mypanel3
    fi
    install -d -m 0750 -o root -g mypanel3 "$ETC_DIR"
    install -d -m 0750 -o mypanel3 -g mypanel3 "$PANEL_DATA_DIR" "${PANEL_DATA_DIR}/backups" "${PANEL_DATA_DIR}/binaries"
}

setup_postgres_db() {
    local existing_url=""
    existing_url="$(read_existing_env "$PANEL_ENV" "DATABASE_URL")"
    local db_pass=""
    if [[ -n "$existing_url" ]]; then
        db_pass="$(printf '%s' "$existing_url" | sed -nE 's|^postgres(ql)?://[^:]+:([^@]+)@.*|\2|p')"
    fi
    if [[ -z "$db_pass" ]]; then
        db_pass="$(openssl rand -hex 24)"
    fi

    if ! su - postgres -c "psql -tAc \"SELECT 1 FROM pg_roles WHERE rolname='mypanel3'\"" | grep -q 1; then
        su - postgres -c "psql -v ON_ERROR_STOP=1 -c \"CREATE ROLE mypanel3 WITH LOGIN PASSWORD '${db_pass}';\""
    else
        su - postgres -c "psql -v ON_ERROR_STOP=1 -c \"ALTER ROLE mypanel3 WITH PASSWORD '${db_pass}';\""
    fi

    if ! su - postgres -c "psql -tAc \"SELECT 1 FROM pg_database WHERE datname='mypanel3'\"" | grep -q 1; then
        su - postgres -c "psql -v ON_ERROR_STOP=1 -c \"CREATE DATABASE mypanel3 OWNER mypanel3;\""
    fi

    DATABASE_URL="postgres://mypanel3:${db_pass}@127.0.0.1:5432/mypanel3"
}

write_panel_env() {
    local jwt_secret=""
    local enc_key=""
    local metrics_token=""
    jwt_secret="$(read_existing_env "$PANEL_ENV" "JWT_SECRET")"
    enc_key="$(read_existing_env "$PANEL_ENV" "ENCRYPTION_KEY")"
    metrics_token="$(read_existing_env "$PANEL_ENV" "METRICS_TOKEN")"

    [[ -n "$jwt_secret" ]] || jwt_secret="$(openssl rand -hex 32)"
    [[ -n "$enc_key" ]] || enc_key="$(openssl rand -hex 32)"
    [[ -n "$metrics_token" ]] || metrics_token="$(openssl rand -hex 24)"

    local api_url="http://127.0.0.1:3000"
    local frontend_url="http://127.0.0.1:3000"
    local sub_url=""
    local cors_origins=""
    local trust_proxy="false"

    if [[ "$PROXY_MODE" == "caddy" ]] && [[ -n "$DOMAIN" ]]; then
        api_url="https://${DOMAIN}"
        frontend_url="https://${DOMAIN}"
        cors_origins="https://${DOMAIN}"
        trust_proxy="true"
        if [[ -n "$SUB_DOMAIN" ]]; then
            sub_url="https://${SUB_DOMAIN}"
            cors_origins="${cors_origins},https://${SUB_DOMAIN}"
        else
            sub_url="$api_url"
        fi
    else
        sub_url="$api_url"
        cors_origins="$api_url"
    fi

    cat > "$PANEL_ENV" <<EOF
DATABASE_URL=${DATABASE_URL}
DB_MAX_CONNECTIONS=10
API_HOST=127.0.0.1
API_PORT=3000
API_URL=${api_url}
SUB_URL=${sub_url}
FRONTEND_URL=${frontend_url}
CORS_ORIGINS=${cors_origins}
NODE_ENV=production
JWT_SECRET=${jwt_secret}
JWT_EXPIRES_IN=7d
ENCRYPTION_KEY=${enc_key}
TRUST_PROXY=${trust_proxy}
RATE_LIMIT_WINDOW=900000
RATE_LIMIT_MAX=300
AUTH_RATE_LIMIT_MAX=20
ALLOW_PRIVATE_NODES=true
NODE_API_PLAINTEXT=true
PROMETHEUS_ENABLED=true
METRICS_TOKEN=${metrics_token}
BACKUP_ENABLED=false
BACKUP_INTERVAL="0 3 * * *"
BACKUP_DIR=/var/lib/mypanel3/backups
BACKUP_RETENTION=14
RUST_LOG=info
LOG_FORMAT=text
DATA_DIR=/var/lib/mypanel3
EOF
    chown root:mypanel3 "$PANEL_ENV"
    chmod 0640 "$PANEL_ENV"
}

write_node_env() {
    install -d -m 0750 -o root -g root "$ETC_DIR" "$NODE_DATA_DIR"

    local secret="$NODE_SECRET"
    if [[ -z "$secret" ]]; then
        secret="$(read_existing_env "$NODE_ENV_FILE" "NODE_RPC_SECRET")"
    fi
    if [[ -z "$secret" ]]; then
        secret="$(openssl rand -hex 24)"
    fi
    NODE_SECRET="$secret"

    local effective_master="$MASTER_URL"
    local effective_node_host="$NODE_DOMAIN"
    local effective_node_name="${NODE_NAME:-$(hostname -f 2>/dev/null || hostname)}"

    if [[ "$MODE" == "both" ]]; then
        effective_master="http://127.0.0.1:3000"
        AGENT_WORKER_HOST="0.0.0.0"
        AGENT_WORKER_PORT="$API_PORT"
        if [[ -z "$effective_node_host" ]]; then
            if [[ -n "$TLS_CERTS_DOMAIN" ]]; then
                effective_node_host="$TLS_CERTS_DOMAIN"
            else
                effective_node_host="$(detect_public_ipv4)"
                effective_node_host="${effective_node_host:-127.0.0.1}"
            fi
        fi
    else
        if [[ -z "$effective_node_host" ]]; then
            effective_node_host="$(detect_public_ipv4)"
            [[ -n "$effective_node_host" ]] || die "Could not determine node host; pass --node-domain <DOMAIN|IP>"
        fi
        case "$TLS_MODE" in
            none)
                AGENT_WORKER_HOST="0.0.0.0"
                AGENT_WORKER_PORT="$API_PORT"
                ;;
            port2087)
                AGENT_WORKER_HOST="127.0.0.1"
                AGENT_WORKER_PORT="$((API_PORT + 1))"
                ;;
            auto|internal)
                AGENT_WORKER_HOST="127.0.0.1"
                AGENT_WORKER_PORT="$API_PORT"
                ;;
        esac
    fi

    NODE_DOMAIN="$effective_node_host"
    NODE_NAME="$effective_node_name"

    cat > "$NODE_ENV_FILE" <<EOF
MASTER_URL=${effective_master}
NODE_RPC_SECRET=${secret}
NODE_HOST=${effective_node_host}
NODE_NAME=${effective_node_name}
WORKER_HOST=${AGENT_WORKER_HOST}
WORKER_PORT=${AGENT_WORKER_PORT}
CONFIG_DIR=${NODE_DATA_DIR}
XRAY_BIN=/usr/local/bin/xray
SINGBOX_BIN=/usr/local/bin/sing-box
NAIVE_BIN=/usr/local/bin/caddy-naive
MIERU_BIN=/usr/local/bin/mita
XRAY_LOCATION_ASSET=/usr/local/share/xray
RUST_LOG=info
LOG_FORMAT=text
EOF
    chown root:root "$NODE_ENV_FILE"
    chmod 0640 "$NODE_ENV_FILE"
}

write_certbot_deploy_hook() {
    local domains_list="$1"
    local inbound_tls_domain="$2"
    local hook_script="/etc/letsencrypt/renewal-hooks/deploy/mypanel3-node-tls.sh"
    install -d -m 0755 "$(dirname "$hook_script")"

    {
        printf '#!/usr/bin/env bash\nset -euo pipefail\n'
        if [[ -n "$inbound_tls_domain" ]]; then
            printf 'install -d -m 0750 -o root -g root "%s"\n' "$NODE_TLS_DIR"
            printf 'cp -L "/etc/letsencrypt/live/%s/fullchain.pem" "%s/fullchain.pem"\n' "$inbound_tls_domain" "$NODE_TLS_DIR"
            printf 'cp -L "/etc/letsencrypt/live/%s/privkey.pem" "%s/privkey.pem"\n' "$inbound_tls_domain" "$NODE_TLS_DIR"
            printf 'chmod 0640 "%s/fullchain.pem" "%s/privkey.pem"\n\n' "$NODE_TLS_DIR" "$NODE_TLS_DIR"
        fi
        if [[ -n "$domains_list" ]]; then
            printf 'install -d -m 0750 -o root -g caddy "%s"\n' "$CADDY_CERTS_DIR"
            printf 'for d in %s; do\n' "$domains_list"
            printf '  if [[ -d "/etc/letsencrypt/live/${d}" ]]; then\n'
            printf '    cp -L "/etc/letsencrypt/live/${d}/fullchain.pem" "%s/${d}.fullchain.pem"\n' "$CADDY_CERTS_DIR"
            printf '    cp -L "/etc/letsencrypt/live/${d}/privkey.pem" "%s/${d}.privkey.pem"\n' "$CADDY_CERTS_DIR"
            printf '    chown root:caddy "%s/${d}.fullchain.pem" "%s/${d}.privkey.pem"\n' "$CADDY_CERTS_DIR" "$CADDY_CERTS_DIR"
            printf '    chmod 0640 "%s/${d}.fullchain.pem" "%s/${d}.privkey.pem"\n' "$CADDY_CERTS_DIR" "$CADDY_CERTS_DIR"
            printf '  fi\n'
            printf 'done\n'
        fi
        printf 'if systemctl is-active --quiet caddy 2>/dev/null; then\n  systemctl reload caddy || true\nfi\n'
    } > "$hook_script"
    chmod 0755 "$hook_script"
    "$hook_script" || true
}

configure_sni_mux_both() {
    # All-in-one SNI-mux topology: HAProxy on <PUBLIC_IP>:443 routes panel domains -> Caddy :9443,
    # and default backend (Reality SNI) -> Xray 127.0.0.1:443. Caddy holds :80 for ACME webroot.
    install_caddy
    install_haproxy
    export DEBIAN_FRONTEND=noninteractive
    apt-get install -y -qq --no-install-recommends certbot

    local pub_ip=""
    pub_ip="$(detect_public_ipv4)"
    [[ -n "$pub_ip" ]] || die "Could not detect public IPv4 for HAProxy :443 bind (use --no-sni-mux if local only)"

    install -d -m 0755 /etc/caddy "$CADDY_CERTS_DIR" /var/www/acme /var/log/caddy

    local all_domains=("$DOMAIN")
    if [[ -n "$SUB_DOMAIN" ]] && [[ "$SUB_DOMAIN" != "$DOMAIN" ]]; then
        all_domains+=("$SUB_DOMAIN")
    fi
    if [[ -n "$TLS_CERTS_DOMAIN" ]] && [[ "$TLS_CERTS_DOMAIN" != "$DOMAIN" ]] && [[ "$TLS_CERTS_DOMAIN" != "$SUB_DOMAIN" ]]; then
        if ! is_ip_addr "$TLS_CERTS_DOMAIN"; then
            all_domains+=("$TLS_CERTS_DOMAIN")
        fi
    fi

    # Stage 1: Temporary HTTP-only Caddyfile for ACME webroot challenges
    : > "$CADDY_FILE"
    {
        if [[ -n "$EMAIL" ]]; then
            printf '{\n\temail %s\n\tauto_https off\n}\n\n' "$EMAIL"
        else
            printf '{\n\tauto_https off\n}\n\n'
        fi
        for d in "${all_domains[@]}"; do
            cat <<EOF
http://${d} {
	@acme path /.well-known/acme-challenge/*
	handle @acme {
		root * /var/www/acme
		file_server
	}
	handle {
		redir https://{host}{uri} 301
	}
}

EOF
        done
    } > "$CADDY_FILE"
    chmod 0644 "$CADDY_FILE"
    chown -R caddy:caddy /var/log/caddy /var/www/acme
    caddy validate --config "$CADDY_FILE"
    systemctl enable --now caddy
    systemctl reload caddy || systemctl restart caddy

    local email_args=("--register-unsafely-without-email")
    if [[ -n "$EMAIL" ]]; then
        email_args=("--email" "$EMAIL")
    fi

    for d in "${all_domains[@]}"; do
        if [[ ! -f "/etc/letsencrypt/live/${d}/fullchain.pem" ]]; then
            log "Issuing Let's Encrypt certificate via ACME webroot for ${d}..."
            certbot certonly --webroot -w /var/www/acme --non-interactive --agree-tos "${email_args[@]}" -d "$d" \
                || die "Certbot certificate issuance failed for ${d}. Ensure DNS A record -> ${pub_ip} and port 80/tcp is open."
        fi
    done

    local inbound_tls="${TLS_CERTS_DOMAIN:-$DOMAIN}"
    is_ip_addr "$inbound_tls" && inbound_tls=""
    write_certbot_deploy_hook "${all_domains[*]}" "$inbound_tls"

    # Stage 2: Full Caddyfile (HTTP ACME + HTTPS panel on 127.0.0.1:9443)
    : > "$CADDY_FILE"
    {
        printf '{\n'
        [[ -n "$EMAIL" ]] && printf '\temail %s\n' "$EMAIL"
        printf '\tauto_https off\n\tservers {\n\t\tprotocols h1 h2\n\t}\n}\n\n'

        for d in "${all_domains[@]}"; do
            cat <<EOF
http://${d} {
	@acme path /.well-known/acme-challenge/*
	handle @acme {
		root * /var/www/acme
		file_server
	}
	handle {
		redir https://{host}{uri} 301
	}
}

EOF
        done

        cat <<EOF
https://${DOMAIN}:9443 {
	bind 127.0.0.1
	tls ${CADDY_CERTS_DIR}/${DOMAIN}.fullchain.pem ${CADDY_CERTS_DIR}/${DOMAIN}.privkey.pem
	encode gzip zstd
	reverse_proxy 127.0.0.1:3000 {
		header_up Host {host}
		header_up X-Real-IP {remote_host}
		header_up X-Forwarded-Proto https
	}
	log {
		output file /var/log/caddy/panel.log
	}
}

EOF
        if [[ -n "$SUB_DOMAIN" ]] && [[ "$SUB_DOMAIN" != "$DOMAIN" ]]; then
            cat <<EOF
https://${SUB_DOMAIN}:9443 {
	bind 127.0.0.1
	tls ${CADDY_CERTS_DIR}/${SUB_DOMAIN}.fullchain.pem ${CADDY_CERTS_DIR}/${SUB_DOMAIN}.privkey.pem
	encode gzip zstd
	reverse_proxy 127.0.0.1:3000 {
		header_up Host {host}
		header_up X-Real-IP {remote_host}
		header_up X-Forwarded-Proto https
	}
	log {
		output file /var/log/caddy/sub.log
	}
}

EOF
        fi
    } > "$CADDY_FILE"

    chmod 0644 "$CADDY_FILE"
    caddy validate --config "$CADDY_FILE"
    chown -R caddy:caddy /var/log/caddy
    systemctl reload caddy || systemctl restart caddy

    # Write HAProxy SNI multiplexer
    local sni_acls=""
    sni_acls="    acl is_panel req.ssl_sni -i ${DOMAIN}"
    if [[ -n "$SUB_DOMAIN" ]] && [[ "$SUB_DOMAIN" != "$DOMAIN" ]]; then
        sni_acls="${sni_acls}"$'\n'"    acl is_panel req.ssl_sni -i ${SUB_DOMAIN}"
    fi

    cat > "$HAPROXY_FILE" <<EOF
global
    log /dev/log local0
    maxconn 8192
    user haproxy
    group haproxy
    daemon
    stats socket /run/haproxy/admin.sock mode 660 level admin
    tune.bufsize 32768

defaults
    log global
    mode tcp
    option tcplog
    timeout connect 10s
    timeout client  300s
    timeout server  300s

frontend fe_https
    bind ${pub_ip}:443
    tcp-request inspect-delay 2s
    tcp-request content accept if { req.ssl_sni -m found }
${sni_acls}
    use_backend be_caddy_sites if is_panel
    default_backend be_xray_reality

backend be_xray_reality
    server xray 127.0.0.1:443

backend be_caddy_sites
    server caddy 127.0.0.1:9443
EOF
    haproxy -c -f "$HAPROXY_FILE"
    systemctl enable --now haproxy
    systemctl reload haproxy || systemctl restart haproxy
}

write_caddy_config() {
    install -d -m 0755 /etc/caddy /var/log/caddy
    : > "$CADDY_FILE"

    {
        printf '{\n'
        [[ -n "$EMAIL" ]] && [[ "$TLS_MODE" != "internal" ]] && printf '\temail %s\n' "$EMAIL"
        if [[ "$MODE" == "node" ]] && [[ "$TLS_MODE" == "port2087" ]]; then
            printf '\thttps_port %s\n' "$API_PORT"
        fi
        printf '}\n\n'
    } > "$CADDY_FILE"

    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        if [[ "$PROXY_MODE" == "caddy" ]] && [[ -n "$DOMAIN" ]]; then
            local tls_directive=""
            if [[ "$TLS_MODE" == "internal" ]]; then
                tls_directive=$'\ttls internal\n'
            fi
            cat >> "$CADDY_FILE" <<EOF
${DOMAIN} {
${tls_directive}	encode gzip zstd
	reverse_proxy 127.0.0.1:3000 {
		header_up Host {host}
		header_up X-Real-IP {remote_host}
		header_up X-Forwarded-Proto {scheme}
	}
	log {
		output file /var/log/caddy/panel.log
	}
}

EOF
            if [[ -n "$SUB_DOMAIN" ]]; then
                cat >> "$CADDY_FILE" <<EOF
${SUB_DOMAIN} {
${tls_directive}	encode gzip zstd
	reverse_proxy 127.0.0.1:3000 {
		header_up Host {host}
		header_up X-Real-IP {remote_host}
		header_up X-Forwarded-Proto {scheme}
	}
	log {
		output file /var/log/caddy/sub.log
	}
}

EOF
            fi
        fi
    fi

    if [[ "$MODE" == "node" ]] && [[ "$TLS_MODE" != "none" ]] && [[ -n "$NODE_DOMAIN" ]]; then
        local node_tls_directive=""
        if [[ "$TLS_MODE" == "internal" ]]; then
            node_tls_directive=$'\ttls internal\n'
        fi
        if [[ "$TLS_MODE" == "port2087" ]]; then
            cat >> "$CADDY_FILE" <<EOF
https://${NODE_DOMAIN}:${API_PORT} {
	reverse_proxy 127.0.0.1:${AGENT_WORKER_PORT} {
		header_up Host {host}
		header_up X-Real-IP {remote_host}
		header_up X-Forwarded-Proto {scheme}
	}
	log {
		output file /var/log/caddy/node.log
	}
}

EOF
        else
            cat >> "$CADDY_FILE" <<EOF
${NODE_DOMAIN} {
${node_tls_directive}	reverse_proxy 127.0.0.1:${AGENT_WORKER_PORT} {
		header_up Host {host}
		header_up X-Real-IP {remote_host}
		header_up X-Forwarded-Proto {scheme}
	}
	log {
		output file /var/log/caddy/node.log
	}
}

EOF
        fi
    fi

    chmod 0644 "$CADDY_FILE"
    caddy validate --config "$CADDY_FILE"
    chown -R caddy:caddy /var/log/caddy
    systemctl enable --now caddy
    systemctl reload caddy || systemctl restart caddy
}

issue_inbound_tls_certs() {
    [[ -n "$TLS_CERTS_DOMAIN" ]] || return 0
    if is_ip_addr "$TLS_CERTS_DOMAIN"; then
        warn "--tls-certs-domain '${TLS_CERTS_DOMAIN}' is an IP address; skipping Let's Encrypt cert issuance."
        return 0
    fi
    if [[ "$MODE" == "both" ]] && [[ "$SNI_MUX" == "true" ]]; then
        # Already issued cleanly via Caddy ACME webroot in configure_sni_mux_both
        return 0
    fi

    log "Issuing Let's Encrypt certificate for inbound TLS (${TLS_CERTS_DOMAIN})..."
    export DEBIAN_FRONTEND=noninteractive
    apt-get install -y -qq --no-install-recommends certbot
    install -d -m 0750 "$NODE_TLS_DIR"

    local email_args=("--register-unsafely-without-email")
    if [[ -n "$EMAIL" ]]; then
        email_args=("--email" "$EMAIL")
    fi

    if [[ ! -f "/etc/letsencrypt/live/${TLS_CERTS_DOMAIN}/fullchain.pem" ]]; then
        local stopped_caddy="false"
        if systemctl is-active --quiet caddy 2>/dev/null; then
            systemctl stop caddy
            stopped_caddy="true"
        fi
        certbot certonly --standalone --preferred-challenges http --non-interactive --agree-tos "${email_args[@]}" -d "$TLS_CERTS_DOMAIN" \
            || warn "Certbot standalone issuance failed for ${TLS_CERTS_DOMAIN}; ensure port 80/tcp is open."
        if [[ "$stopped_caddy" == "true" ]]; then
            systemctl start caddy
        fi
    fi

    if [[ -f "/etc/letsencrypt/live/${TLS_CERTS_DOMAIN}/fullchain.pem" ]]; then
        write_certbot_deploy_hook "" "$TLS_CERTS_DOMAIN"
    fi
}

install_systemd_units() {
    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        cat > "${SYSTEMD_DIR}/mypanel3.service" <<'EOF'
[Unit]
Description=mypanel3 proxy management panel
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=simple
User=mypanel3
Group=mypanel3
WorkingDirectory=/var/lib/mypanel3
EnvironmentFile=/etc/mypanel3/mypanel3.env
ExecStart=/usr/local/bin/mypanel3 serve
Restart=on-failure
RestartSec=5s
LimitNOFILE=65535
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/mypanel3

[Install]
WantedBy=multi-user.target
EOF
        chmod 0644 "${SYSTEMD_DIR}/mypanel3.service"
    fi

    if [[ "$MODE" == "node" ]] || [[ "$MODE" == "both" ]]; then
        cat > "${SYSTEMD_DIR}/mypanel3-agent.service" <<'EOF'
[Unit]
Description=mypanel3 node worker agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
Group=root
WorkingDirectory=/var/lib/mypanel3-node
EnvironmentFile=/etc/mypanel3/node.env
ExecStart=/usr/local/bin/mypanel3 agent
Restart=always
RestartSec=5s
LimitNOFILE=1048576
PrivateTmp=true
ProtectHome=true

[Install]
WantedBy=multi-user.target
EOF
        chmod 0644 "${SYSTEMD_DIR}/mypanel3-agent.service"
    fi

    systemctl daemon-reload
}

bootstrap_admin() {
    if [[ -f "$ADMIN_CREDS_FILE" ]] && [[ -z "$ADMIN_PASS" ]] && [[ "$GEN_ADMIN_PASS" != "true" ]]; then
        "$BIN_DEST" --env-file "$PANEL_ENV" admin create --username "$ADMIN_USER" --if-missing >/dev/null || true
        return 0
    fi

    local pass="$ADMIN_PASS"
    if [[ -z "$pass" ]]; then
        pass="$(openssl rand -base64 18 | tr -dc 'A-Za-z0-9!@#%^&*_-' | head -c 20)"
    fi

    local email_flag=()
    if [[ -n "$EMAIL" ]]; then
        email_flag=("--email" "$EMAIL")
    fi

    "$BIN_DEST" --env-file "$PANEL_ENV" admin create \
        --username "$ADMIN_USER" \
        --password "$pass" \
        "${email_flag[@]}" \
        --if-missing

    cat > "$ADMIN_CREDS_FILE" <<EOF
username=${ADMIN_USER}
password=${pass}
EOF
    chown root:root "$ADMIN_CREDS_FILE"
    chmod 0600 "$ADMIN_CREDS_FILE"
}

register_local_node_in_db() {
    if [[ "$MODE" != "both" ]]; then
        return 0
    fi
    local secret=""
    secret="$(read_existing_env "$NODE_ENV_FILE" "NODE_RPC_SECRET")"
    local host=""
    host="$(read_existing_env "$NODE_ENV_FILE" "NODE_HOST")"
    local name=""
    name="$(read_existing_env "$NODE_ENV_FILE" "NODE_NAME")"
    local api_p=""
    api_p="$(read_existing_env "$NODE_ENV_FILE" "WORKER_PORT")"
    [[ -n "$secret" ]] || return 0
    host="${host:-127.0.0.1}"
    name="${name:-local}"
    api_p="${api_p:-2087}"

    log "Registering local node '${name}' (${host}, apiPort=${api_p}) in panel database..."
    "$BIN_DEST" --env-file "$PANEL_ENV" node create \
        --name "$name" \
        --host "$host" \
        --port 443 \
        --api-port "$api_p" \
        --secret "$secret" \
        --if-missing
}

run_wizard() {
    if [[ -z "$MODE" ]]; then
        local choice=""
        choice="$(prompt_line "Select scenario: 1) Panel + Node (all-in-one)  2) Panel only  3) Node only (pair with panel)  4) Upgrade existing" "1")"
        case "$choice" in
            1|both) MODE="both" ;;
            2|panel) MODE="panel" ;;
            3|node) MODE="node" ;;
            4|upgrade)
                UPGRADE_ONLY="true"
                run_upgrade
                exit 0
                ;;
            *) die "Invalid topology choice: $choice" ;;
        esac
    fi

    case "$MODE" in
        panel|node|both) ;;
        *) die "Invalid --mode '$MODE' (expected both, panel, or node)" ;;
    esac

    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        if [[ "$PROXY_MODE" != "none" ]] && [[ -z "$DOMAIN" ]]; then
            DOMAIN="$(prompt_line "Panel domain (e.g. panel.example.com, leave empty for plain HTTP :3000)" "")"
            if [[ -z "$DOMAIN" ]]; then
                PROXY_MODE="none"
            fi
        fi
        if [[ -n "$DOMAIN" ]] && ! validate_domain "$DOMAIN"; then
            die "Invalid panel domain: $DOMAIN"
        fi
        if [[ "$PROXY_MODE" != "none" ]] && [[ -z "$SUB_DOMAIN" ]] && [[ "$NON_INTERACTIVE" != "true" ]]; then
            SUB_DOMAIN="$(prompt_line "Optional separate subscription domain (leave empty to share panel domain)" "")"
        fi
        if [[ -n "$SUB_DOMAIN" ]] && ! validate_domain "$SUB_DOMAIN"; then
            die "Invalid subscription domain: $SUB_DOMAIN"
        fi
        if [[ "$PROXY_MODE" != "none" ]] && [[ "$TLS_MODE" == "auto" ]] && [[ -z "$EMAIL" ]]; then
            EMAIL="$(prompt_line "ACME / admin email" "")"
        fi
        if [[ -z "$ADMIN_PASS" ]] && [[ "$GEN_ADMIN_PASS" != "true" ]] && [[ "$NON_INTERACTIVE" != "true" ]] && [[ ! -f "$ADMIN_CREDS_FILE" ]]; then
            local p1=""
            local p2=""
            p1="$(prompt_secret "Admin password (min 12 chars, leave empty to auto-generate)")"
            if [[ -n "$p1" ]]; then
                [[ "${#p1}" -ge 12 ]] || die "Admin password must be at least 12 characters"
                if [[ -z "$ANSWERS_FD" ]]; then
                    p2="$(prompt_secret "Confirm admin password")"
                    [[ "$p1" == "$p2" ]] || die "Admin passwords do not match"
                fi
                ADMIN_PASS="$p1"
            else
                GEN_ADMIN_PASS="true"
            fi
        elif [[ -n "$ADMIN_PASS" ]] && [[ "${#ADMIN_PASS}" -lt 12 ]]; then
            die "Admin password must be at least 12 characters"
        fi

        if [[ "$MODE" == "both" ]] && [[ -z "$NODE_DOMAIN" ]] && [[ "$NON_INTERACTIVE" != "true" ]]; then
            local det_ip=""
            det_ip="$(detect_public_ipv4)"
            NODE_DOMAIN="$(prompt_line "Node public domain or IP for proxy clients" "${det_ip:-$DOMAIN}")"
        fi
        if [[ "$MODE" == "both" ]] && [[ -z "$NODE_NAME" ]] && [[ "$NON_INTERACTIVE" != "true" ]]; then
            NODE_NAME="$(prompt_line "Local node display name" "$(hostname -s 2>/dev/null || hostname)")"
        fi
    fi

    if [[ "$MODE" == "node" ]]; then
        if [[ "$TLS_MODE_EXPLICIT" != "true" ]]; then
            TLS_MODE="none"
        fi
        if [[ -z "$MASTER_URL" ]]; then
            MASTER_URL="$(normalize_master_url "$(prompt_line "Master panel URL (e.g. https://panel.example.com)" "")")"
        fi
        [[ -n "$MASTER_URL" ]] || die "--master-url is required for --mode node"

        if [[ -z "$NODE_DOMAIN" ]]; then
            local det_ip=""
            det_ip="$(detect_public_ipv4)"
            NODE_DOMAIN="$(prompt_line "This node's public domain or IP" "$det_ip")"
        fi
        [[ -n "$NODE_DOMAIN" ]] || die "--node-domain is required for --mode node"

        if [[ -z "$NODE_SECRET" ]]; then
            NODE_SECRET="$(prompt_secret "Node RPC secret (from Panel -> Nodes -> Add node)")"
        fi
        [[ -n "$NODE_SECRET" ]] || die "--node-secret is required for --mode node"

        if is_ip_addr "$NODE_DOMAIN" && { [[ "$TLS_MODE" == "auto" ]] || [[ "$TLS_MODE" == "port2087" ]]; }; then
            warn "Node host '${NODE_DOMAIN}' is an IP address; switching --tls-mode to 'none' (port :${API_PORT}, port 443 free)."
            TLS_MODE="none"
        fi
    fi

    if [[ "$SNI_MUX" == "auto" ]]; then
        if [[ "$MODE" == "both" ]] && [[ "$PROXY_MODE" == "caddy" ]] && [[ -n "$DOMAIN" ]]; then
            SNI_MUX="true"
        else
            SNI_MUX="false"
        fi
    fi

    if [[ "$ENABLE_BBR" == "auto" ]]; then
        if [[ "$MODE" == "node" ]] || [[ "$MODE" == "both" ]]; then
            ENABLE_BBR="true"
        else
            ENABLE_BBR="false"
        fi
    fi
}

run_upgrade() {
    log "Running upgrade..."
    command -v curl >/dev/null 2>&1 || install_base_packages
    install_binary
    if [[ -f "$PANEL_ENV" ]]; then
        log "Running database migrations..."
        "$BIN_DEST" --env-file "$PANEL_ENV" migrate
    fi
    if systemctl list-unit-files | grep -q '^mypanel3\.service'; then
        systemctl restart mypanel3
    fi
    if systemctl list-unit-files | grep -q '^mypanel3-agent\.service'; then
        systemctl restart mypanel3-agent
    fi
    log "Upgrade complete."
}

verify_installation() {
    local ok=true
    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        log "Verifying panel health endpoint (http://127.0.0.1:3000/api/health)..."
        local ready="false"
        for _ in $(seq 1 15); do
            if curl -fsS http://127.0.0.1:3000/api/health 2>/dev/null | grep -q '"ok"'; then
                ready="true"
                break
            fi
            sleep 1
        done
        if [[ "$ready" == "true" ]]; then
            log "Panel healthcheck: OK"
        else
            warn "Panel did not report ready within 15s (check: journalctl -u mypanel3 -n 50)"
            ok=false
        fi
    fi

    if [[ "$MODE" == "node" ]] || [[ "$MODE" == "both" ]]; then
        local check_port="$AGENT_WORKER_PORT"
        local node_sec=""
        node_sec="$(read_existing_env "$NODE_ENV_FILE" "NODE_RPC_SECRET")"
        log "Verifying node worker agent (http://127.0.0.1:${check_port}/api/status)..."
        local agent_ready="false"
        for _ in $(seq 1 15); do
            if curl -fsS -H "Authorization: Bearer ${node_sec}" "http://127.0.0.1:${check_port}/api/status" >/dev/null 2>&1; then
                agent_ready="true"
                break
            fi
            sleep 1
        done
        if [[ "$agent_ready" == "true" ]]; then
            log "Node worker agent: OK"
        else
            warn "Node agent did not report ready within 15s (check: journalctl -u mypanel3-agent -n 50)"
            ok=false
        fi
    fi

    [[ "$ok" == "true" ]] || true
}

print_summary() {
    printf '\n'
    log "Installation complete (${MODE})."
    printf '------------------------------------------------------------\n'
    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        if [[ "$PROXY_MODE" == "caddy" ]] && [[ -n "$DOMAIN" ]]; then
            printf '  Panel URL:        https://%s\n' "$DOMAIN"
            if [[ -n "$SUB_DOMAIN" ]]; then
                printf '  Subscription URL: https://%s\n' "$SUB_DOMAIN"
            fi
        else
            printf '  Panel URL:        http://127.0.0.1:3000\n'
        fi
        printf '  Credentials file: %s\n' "$ADMIN_CREDS_FILE"
        if [[ -f "$ADMIN_CREDS_FILE" ]]; then
            local u p
            u="$(grep '^username=' "$ADMIN_CREDS_FILE" | cut -d'=' -f2- || true)"
            p="$(grep '^password=' "$ADMIN_CREDS_FILE" | cut -d'=' -f2- || true)"
            printf '  Admin login:      %s / %s\n' "$u" "$p"
        fi
        printf '  Panel env:        %s\n' "$PANEL_ENV"
    fi
    if [[ "$MODE" == "node" ]] || [[ "$MODE" == "both" ]]; then
        local node_sec=""
        node_sec="$(read_existing_env "$NODE_ENV_FILE" "NODE_RPC_SECRET")"
        printf '  Node env:         %s\n' "$NODE_ENV_FILE"
        printf '  Node host/name:   %s (%s)\n' "$NODE_DOMAIN" "$NODE_NAME"
        printf '  Agent API port:   %s (tls-mode=%s)\n' "$AGENT_WORKER_PORT" "$TLS_MODE"
        if [[ "$MODE" == "both" ]]; then
            if [[ "$SNI_MUX" == "true" ]]; then
                printf '  SNI Multiplexer:  HAProxy %s:443 -> Panel (Caddy :9443) + Xray (:443)\n' "${PUBLIC_IP}"
            fi
            printf '  Local node:       Auto-registered in DB (host=%s, apiPort=%s, secret=%s)\n' \
                "$NODE_DOMAIN" "$AGENT_WORKER_PORT" "$node_sec"
        else
            printf '  Master Panel:     %s (connecting over outbound WSS)\n' "$MASTER_URL"
            printf '  Port 443 status:  %s\n' "$( [[ "$TLS_MODE" == "none" || "$TLS_MODE" == "port2087" ]] && echo 'FREE for VLESS Reality / proxy inbounds' || echo 'Used by Caddy' )"
        fi
        printf '  Proxy cores:\n'
        for b in xray sing-box caddy-naive mita; do
            if [[ -x "/usr/local/bin/${b}" ]]; then
                printf '    - %-12s /usr/local/bin/%s [OK]\n' "$b" "$b"
            else
                printf '    - %-12s not installed\n' "$b"
            fi
        done
    fi
    printf '  Logs:             journalctl -u mypanel3 -f | journalctl -u mypanel3-agent -f\n'
    printf '------------------------------------------------------------\n\n'
}

main() {
    parse_args "$@"
    check_root
    check_os_and_arch

    if [[ "$UPGRADE_ONLY" == "true" ]]; then
        run_upgrade
        exit 0
    fi

    init_answers_fd
    run_wizard
    ensure_swap
    install_base_packages
    install_binary
    configure_bbr

    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        check_port_free 5432 postgres
        check_port_free 3000 mypanel3
        if [[ "$PROXY_MODE" == "caddy" ]] && [[ -n "$DOMAIN" ]]; then
            check_port_free 80 caddy
            if [[ "$SNI_MUX" == "true" ]]; then
                check_port_free 9443 caddy
            fi
            check_port_free 443 caddy
            check_dns_record "$DOMAIN"
            [[ -n "$SUB_DOMAIN" ]] && check_dns_record "$SUB_DOMAIN"
        fi
        install_postgres
        ensure_panel_user_and_dirs
        setup_postgres_db
        write_panel_env
        log "Running database migrations..."
        "$BIN_DEST" --env-file "$PANEL_ENV" migrate
        bootstrap_admin
    fi

    if [[ "$MODE" == "node" ]] || [[ "$MODE" == "both" ]]; then
        install_proxy_cores
        write_node_env
        check_port_free "$AGENT_WORKER_PORT" mypanel3
        if [[ "$MODE" == "node" ]]; then
            if [[ "$TLS_MODE" == "port2087" ]]; then
                check_port_free 80 caddy
                check_port_free "$API_PORT" caddy
                check_dns_record "$NODE_DOMAIN"
            elif [[ "$TLS_MODE" == "auto" ]] || [[ "$TLS_MODE" == "internal" ]]; then
                check_port_free 80 caddy
                check_port_free 443 caddy
                check_dns_record "$NODE_DOMAIN"
            fi
        fi
    fi

    if [[ "$MODE" == "both" ]] && [[ "$SNI_MUX" == "true" ]] && [[ -n "$DOMAIN" ]]; then
        configure_sni_mux_both
    elif { [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; } && [[ "$PROXY_MODE" == "caddy" ]] && [[ -n "$DOMAIN" ]]; then
        install_caddy
        write_caddy_config
    elif [[ "$MODE" == "node" ]] && [[ "$TLS_MODE" != "none" ]]; then
        install_caddy
        write_caddy_config
    fi

    if [[ -n "$TLS_CERTS_DOMAIN" ]]; then
        issue_inbound_tls_certs
    fi

    register_local_node_in_db
    install_systemd_units

    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        systemctl enable --now mypanel3
    fi
    if [[ "$MODE" == "node" ]] || [[ "$MODE" == "both" ]]; then
        systemctl enable --now mypanel3-agent
    fi

    verify_installation
    print_summary
}

main "$@"
