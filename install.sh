#!/usr/bin/env bash
set -euo pipefail

# mypanel3 (ProxyPanel v4) — единый интерактивный и идемпотентный установщик
# Поддерживаемые ОС: Debian 12 (и 11+), Ubuntu 22.04–24.04 (и 20.04+) [amd64 / arm64]
# Режимы развертывания:
#   1) Панель + локальная нода на одном сервере (--mode both)
#   2) Только панель (--mode panel)
#   3) Только нода — внешний сервер (--mode node, порт управления 2222, :443 свободен)
#   4) Обновление существующей установки (--upgrade)

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
TLS_CERTS_DOMAIN_EXPLICIT="false"
ADMIN_USER="admin"
ADMIN_USER_EXPLICIT="false"
ADMIN_PASS=""
GEN_ADMIN_PASS="false"
MASTER_URL=""
PANEL_IP=""
NODE_DOMAIN=""
NODE_NAME=""
NODE_SECRET=""
API_PORT="2222"
API_PORT_EXPLICIT="false"
CORES_SPEC="all"
ENABLE_BBR="auto"
CONFIGURE_UFW="true"
RECONFIGURE="false"
NON_INTERACTIVE="false"
UPGRADE_ONLY="false"
BINARY_PATH=""
BINARY_URL=""
ANSWERS_FILE=""
GITHUB_REPO="${GITHUB_REPO:-Velio322/proxy-panel-v4}"

ARCH=""
OS_NAME=""
PUBLIC_IP=""
AGENT_WORKER_PORT="2222"
AGENT_WORKER_HOST="0.0.0.0"

PANEL_STATUS="not-installed"
AGENT_STATUS="not-installed"
LOCAL_NODE_STATUS="not-configured"
UFW_STATUS="skipped"

TMP_DIRS=()
make_tmp_dir() {
    local d=""
    d="$(mktemp -d)"
    TMP_DIRS+=("$d")
    printf '%s' "$d"
}

cleanup_tmp() {
    if [[ ${#TMP_DIRS[@]} -gt 0 ]]; then
        local d=""
        for d in "${TMP_DIRS[@]}"; do
            [[ -n "$d" && -d "$d" ]] && rm -rf "$d" || true
        done
    fi
}
trap cleanup_tmp EXIT

C_RESET=""
C_BOLD=""
C_DIM=""
C_RED=""
C_GREEN=""
C_YELLOW=""
C_BLUE=""
C_CYAN=""

init_colors() {
    if [[ -t 1 ]] && [[ -z "${NO_COLOR:-}" ]]; then
        C_RESET=$'\033[0m'
        C_BOLD=$'\033[1m'
        C_DIM=$'\033[2m'
        C_RED=$'\033[31m'
        C_GREEN=$'\033[32m'
        C_YELLOW=$'\033[33m'
        C_BLUE=$'\033[34m'
        C_CYAN=$'\033[36m'
    fi
}

log() {
    printf '%s[mypanel3]%s %s\n' "$C_CYAN" "$C_RESET" "$*"
}

ok() {
    printf '%s[OK]%s %s\n' "$C_GREEN" "$C_RESET" "$*"
}

step() {
    printf '\n%s%s==> %s%s\n' "$C_BOLD" "$C_BLUE" "$*" "$C_RESET"
}

warn() {
    printf '%s[WARN]%s %s\n' "$C_YELLOW" "$C_RESET" "$*" >&2
}

die() {
    printf '%s[ERROR]%s %s\n' "$C_RED" "$C_RESET" "$*" >&2
    exit 1
}

print_banner() {
    printf '%s%s' "$C_BOLD" "$C_CYAN"
    cat <<'EOF'
╔══════════════════════════════════════════════════════════════════════╗
║   ProxyPanel v4 (mypanel3) — Установщик панели и нод                ║
EOF
    printf '╚══════════════════════════════════════════════════════════════════════╝%s\n' "$C_RESET"
    if [[ -n "$OS_NAME" || -n "$ARCH" || -n "$PUBLIC_IP" ]]; then
        printf '%s  Система: %s | Архитектура: %s | IP: %s%s\n\n' \
            "$C_DIM" "${OS_NAME:-unknown}" "${ARCH:-unknown}" "${PUBLIC_IP:-unknown}" "$C_RESET"
    fi
}

usage() {
    cat <<'EOF'
Использование: sudo bash install.sh [ПАРАМЕТРЫ]

Режимы развертывания:
  --mode <both|panel|node>      Сценарий установки:
                                  both  — Панель + локальная нода на одном сервере
                                  panel — Только управляющая панель + PostgreSQL + Caddy
                                  node  — Только нода (агент + прокси-ядра, :443 свободен)
  --upgrade                     Обновить бинарник, выполнить миграции и перезапустить службы
  --reconfigure                 Принудительно перезаписать конфиги при повторном запуске
  --binary <PATH>               Путь к локальному бинарнику mypanel3
  --binary-url <URL>            Прямой URL для скачивания бинарника mypanel3

Параметры панели:
  --domain <DOMAIN>             Домен панели (например, panel.example.com)
  --sub-domain <DOMAIN>         Отдельный домен для подписок (опционально)
  --email, --admin-email <ADDR> Email для Let's Encrypt и администратора
  --proxy <caddy|none>          Reverse proxy для панели (по умолчанию: caddy)
  --sni-mux / --no-sni-mux      В режиме both: делить :443 между панелью и Reality через HAProxy
  --admin-user <USER>           Логин администратора (по умолчанию: admin)
  --admin-pass <PASS>           Пароль администратора (минимум 12 символов)
  --gen-admin-pass              Сгенерировать случайный пароль администратора

Параметры ноды:
  --master-url, --panel-url <URL|DOMAIN>
                                Адрес управляющей панели (например, https://panel.example.com)
  --panel-ip <IP[,CIDR...]>     IP/CIDR управляющей панели (для UFW и привязки порта агента)
  --node-domain <DOMAIN|IP>     Домен или публичный IPv4/IPv6 ноды
  --node-name <NAME>            Отображаемое имя ноды (по умолчанию: hostname)
  --node-secret <SECRET>        Секрет ноды NODE_SECRET (16–256 символов из веб-интерфейса панели)
  --api-port <PORT>             Порт управления нодой (по умолчанию: 2222)
  --tls-mode <none|port2222|auto|internal>
                                Транспорт API ноды (по умолчанию для node: none на :2222, :443 свободен;
                                port2222/port2087 — Caddy HTTPS на порту API -> loopback)
  --tls-certs-domain <DOMAIN>   Выпустить Let's Encrypt сертификаты в /etc/mypanel3-node/tls
  --cores <all|list|none>       Прокси-ядра: xray,singbox,naive,mieru (по умолчанию: all)
  --skip-cores                  Синоним --cores none
  --bbr / --no-bbr              Настройка fq + TCP BBR (по умолчанию включено для node/both)
  --no-ufw                      Не изменять правила UFW

Автоматизация:
  --answers <FILE|->            Читать ответы мастера построчно из файла или stdin
  -y, --yes, --non-interactive  Неинтерактивный режим (значения по умолчанию, авто-пароли)
  -h, --help                    Показать эту справку
EOF
}

trim_ws() {
    local v="$1"
    v="${v#"${v%%[![:space:]]*}"}"
    v="${v%"${v##*[![:space:]]}"}"
    printf '%s' "$v"
}

normalize_master_url() {
    local u=""
    u="$(trim_ws "$1")"
    u="${u%/}"
    u="${u%/ws/worker}"
    if [[ -z "$u" ]]; then
        return 0
    fi
    if [[ "$u" == wss://* ]]; then
        u="https://${u#wss://}"
    elif [[ "$u" == ws://* ]]; then
        u="http://${u#ws://}"
    elif [[ "$u" != https://* ]] && [[ "$u" != http://* ]]; then
        u="https://${u}"
    fi
    printf '%s' "$u"
}

extract_host_from_url() {
    local u="$1"
    u="${u#https://}"
    u="${u#http://}"
    u="${u#wss://}"
    u="${u#ws://}"
    u="${u%%/*}"
    if [[ "$u" == \[*\]* ]]; then
        u="${u#\[}"
        u="${u%%\]*}"
    else
        u="${u%%:*}"
    fi
    printf '%s' "$u"
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --mode)
                [[ $# -ge 2 ]] || die "--mode требует значение"
                MODE="$2"
                shift 2
                ;;
            --domain)
                [[ $# -ge 2 ]] || die "--domain требует значение"
                DOMAIN="$2"
                shift 2
                ;;
            --sub-domain)
                [[ $# -ge 2 ]] || die "--sub-domain требует значение"
                SUB_DOMAIN="$2"
                shift 2
                ;;
            --email|--admin-email)
                [[ $# -ge 2 ]] || die "$1 требует значение"
                EMAIL="$2"
                shift 2
                ;;
            --proxy)
                [[ $# -ge 2 ]] || die "--proxy требует значение"
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
                [[ $# -ge 2 ]] || die "--tls-mode требует значение"
                TLS_MODE="$2"
                TLS_MODE_EXPLICIT="true"
                shift 2
                ;;
            --tls-certs-domain)
                [[ $# -ge 2 ]] || die "--tls-certs-domain требует значение"
                TLS_CERTS_DOMAIN="$2"
                TLS_CERTS_DOMAIN_EXPLICIT="true"
                shift 2
                ;;
            --admin-user)
                [[ $# -ge 2 ]] || die "--admin-user требует значение"
                ADMIN_USER="$2"
                ADMIN_USER_EXPLICIT="true"
                shift 2
                ;;
            --admin-pass)
                [[ $# -ge 2 ]] || die "--admin-pass требует значение"
                ADMIN_PASS="$2"
                shift 2
                ;;
            --gen-admin-pass)
                GEN_ADMIN_PASS="true"
                shift
                ;;
            --master-url|--panel-url)
                [[ $# -ge 2 ]] || die "$1 требует значение"
                MASTER_URL="$(normalize_master_url "$2")"
                shift 2
                ;;
            --panel-ip)
                [[ $# -ge 2 ]] || die "--panel-ip требует значение"
                PANEL_IP="$2"
                shift 2
                ;;
            --node-domain|--node-host)
                [[ $# -ge 2 ]] || die "$1 требует значение"
                NODE_DOMAIN="$2"
                shift 2
                ;;
            --node-name)
                [[ $# -ge 2 ]] || die "--node-name требует значение"
                NODE_NAME="$2"
                shift 2
                ;;
            --node-secret)
                [[ $# -ge 2 ]] || die "--node-secret требует значение"
                NODE_SECRET="$2"
                shift 2
                ;;
            --node-id)
                [[ $# -ge 2 ]] || die "--node-id требует значение"
                shift 2
                ;;
            --api-port)
                [[ $# -ge 2 ]] || die "--api-port требует значение"
                API_PORT="$2"
                API_PORT_EXPLICIT="true"
                shift 2
                ;;
            --cores)
                [[ $# -ge 2 ]] || die "--cores требует значение"
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
            --no-ufw)
                CONFIGURE_UFW="false"
                shift
                ;;
            --reconfigure)
                RECONFIGURE="true"
                shift
                ;;
            --answers)
                [[ $# -ge 2 ]] || die "--answers требует значение"
                ANSWERS_FILE="$2"
                shift 2
                ;;
            --upgrade)
                UPGRADE_ONLY="true"
                shift
                ;;
            --binary)
                [[ $# -ge 2 ]] || die "--binary требует значение"
                BINARY_PATH="$2"
                shift 2
                ;;
            --binary-url)
                [[ $# -ge 2 ]] || die "--binary-url требует значение"
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
            --)
                shift
                ;;
            *)
                die "Неизвестный параметр: $1 (используйте --help)"
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
            [[ -f "$ANSWERS_FILE" ]] || die "Файл ответов не найден: $ANSWERS_FILE"
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
        result="$(trim_ws "$result")"
        printf '%s' "${result:-$default_val}"
        return
    fi

    if [[ "$NON_INTERACTIVE" == "true" ]]; then
        printf '%s' "$default_val"
        return
    fi

    if [[ -n "$default_val" ]]; then
        printf '%s%s%s [%s]: ' "$C_BOLD" "$prompt_text" "$C_RESET" "$default_val" >&2
    else
        printf '%s%s%s: ' "$C_BOLD" "$prompt_text" "$C_RESET" >&2
    fi
    if [[ ! -t 0 ]] && [[ -r /dev/tty ]]; then
        IFS= read -r result </dev/tty || result=""
    else
        IFS= read -r result || result=""
    fi
    result="$(trim_ws "$result")"
    printf '%s' "${result:-$default_val}"
}

prompt_secret() {
    local prompt_text="$1"
    local result=""

    if [[ -n "$ANSWERS_FD" ]]; then
        IFS= read -r -u "$ANSWERS_FD" result || result=""
        printf '%s' "$(trim_ws "$result")"
        return
    fi

    if [[ "$NON_INTERACTIVE" == "true" ]]; then
        printf '%s' ""
        return
    fi

    printf '%s%s%s: ' "$C_BOLD" "$prompt_text" "$C_RESET" >&2
    if [[ ! -t 0 ]] && [[ -r /dev/tty ]]; then
        IFS= read -r -s result </dev/tty || result=""
    else
        IFS= read -r -s result || result=""
    fi
    printf '\n' >&2
    printf '%s' "$(trim_ws "$result")"
}

prompt_confirm() {
    local prompt_text="$1"
    local default_yes="${2:-Y}"
    local hint="Y/n"
    [[ "$default_yes" != "Y" ]] && hint="y/N"

    if [[ "$NON_INTERACTIVE" == "true" ]] || [[ -n "$ANSWERS_FD" ]]; then
        [[ "$default_yes" == "Y" ]]
        return
    fi

    local ans=""
    printf '%s%s [%s]: %s' "$C_YELLOW" "$prompt_text" "$hint" "$C_RESET" >&2
    if [[ ! -t 0 ]] && [[ -r /dev/tty ]]; then
        IFS= read -r ans </dev/tty || ans=""
    else
        IFS= read -r ans || ans=""
    fi
    ans="$(trim_ws "$ans")"
    if [[ -z "$ans" ]]; then
        [[ "$default_yes" == "Y" ]]
        return
    fi
    case "$ans" in
        y|Y|yes|YES|д|Д|да|ДА) return 0 ;;
        *) return 1 ;;
    esac
}

check_root() {
    if [[ "$(id -u)" -ne 0 ]]; then
        printf '%s[ERROR]%s Скрипт должен запускаться с правами root.\n' "$C_RED" "$C_RESET" >&2
        printf 'Выполните:\n  sudo bash <(curl -fsSL https://raw.githubusercontent.com/%s/main/install.sh)\n' "$GITHUB_REPO" >&2
        exit 1
    fi
}

check_os_and_arch() {
    [[ -f /etc/os-release ]] || die "Не удалось определить ОС (/etc/os-release отсутствует)"
    local os_id=""
    local os_ver=""
    local pretty=""
    os_id="$(sed -nE 's/^ID="?([^"]+)"?/\1/p' /etc/os-release | head -n 1)"
    os_ver="$(sed -nE 's/^VERSION_ID="?([^"]+)"?/\1/p' /etc/os-release | head -n 1)"
    pretty="$(sed -nE 's/^PRETTY_NAME="?([^"]+)"?/\1/p' /etc/os-release | head -n 1)"
    os_ver="${os_ver:-0}"
    OS_NAME="${pretty:-${os_id} ${os_ver}}"

    case "$os_id" in
        debian)
            if [[ "${os_ver%%.*}" -lt 11 ]]; then
                die "Требуется Debian 11+ (рекомендуется Debian 12; обнаружен $os_ver)"
            elif [[ "${os_ver%%.*}" -lt 12 ]]; then
                warn "Обнаружен Debian $os_ver (официально поддерживается Debian 12+); продолжаем."
            fi
            ;;
        ubuntu)
            if [[ "${os_ver%%.*}" -lt 20 ]]; then
                die "Требуется Ubuntu 20.04+ (рекомендуется Ubuntu 22.04–24.04; обнаружена $os_ver)"
            elif [[ "${os_ver%%.*}" -lt 22 ]]; then
                warn "Обнаружена Ubuntu $os_ver (рекомендуется Ubuntu 22.04–24.04); продолжаем."
            fi
            ;;
        *)
            warn "Нетестированная ОС '$os_id'; продолжаем на ваш страх и риск."
            ;;
    esac

    local arch=""
    arch="$(uname -m)"
    case "$arch" in
        x86_64|amd64) ARCH="amd64" ;;
        aarch64|arm64) ARCH="arm64" ;;
        *) die "Неподдерживаемая архитектура: $arch (поддерживаются: amd64, arm64)" ;;
    esac

    if ! command -v systemctl >/dev/null 2>&1; then
        die "Требуется systemd (команда systemctl не найдена)"
    fi
}

ensure_curl() {
    if ! command -v curl >/dev/null 2>&1; then
        log "Устанавливаю curl..."
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        apt-get install -y -qq --no-install-recommends ca-certificates curl
    fi
}

detect_public_ipv4() {
    if [[ -n "$PUBLIC_IP" ]]; then
        printf '%s' "$PUBLIC_IP"
        return
    fi
    local ip=""
    if command -v curl >/dev/null 2>&1; then
        ip="$(curl -4fsS --max-time 4 https://api.ipify.org 2>/dev/null || curl -4fsS --max-time 4 https://ifconfig.me 2>/dev/null || true)"
    fi
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
    if [[ "$v" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}(/[0-9]{1,2})?$ ]]; then
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

validate_port() {
    local p="$1"
    [[ "$p" =~ ^[0-9]+$ ]] && [[ "$p" -ge 1 ]] && [[ "$p" -le 65535 ]]
}

validate_node_secret() {
    local s="$1"
    [[ "${#s}" -ge 16 ]] && [[ "${#s}" -le 256 ]] && [[ ! "$s" =~ [[:space:]] ]]
}

resolve_domain_a() {
    local fqdn="$1"
    local ips=""
    if command -v dig >/dev/null 2>&1; then
        ips="$(dig +short A "$fqdn" 2>/dev/null | grep -E '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$' || true)"
    fi
    if [[ -z "$ips" ]] && command -v getent >/dev/null 2>&1; then
        ips="$(getent ahostsv4 "$fqdn" 2>/dev/null | awk '{print $1}' | sort -u || true)"
    fi
    if [[ -z "$ips" ]] && command -v host >/dev/null 2>&1; then
        ips="$(host -t A "$fqdn" 2>/dev/null | awk '/has address/ {print $4}' || true)"
    fi
    printf '%s' "$ips"
}

check_dns_record() {
    local fqdn="$1"
    [[ -n "$fqdn" ]] || return 0
    if is_ip_addr "$fqdn"; then
        return 0
    fi

    local resolved=""
    resolved="$(resolve_domain_a "$fqdn")"
    local pub_ip=""
    pub_ip="$(detect_public_ipv4)"
    local local_ips=""
    local_ips="$(hostname -I 2>/dev/null || true)"

    if [[ -z "$resolved" ]]; then
        warn "DNS A-запись для '$fqdn' не найдена (текущий IP сервера: ${pub_ip:-неизвестен})."
        return 1
    fi

    local matched="false"
    local r=""
    while IFS= read -r r; do
        [[ -n "$r" ]] || continue
        if [[ -n "$pub_ip" && "$r" == "$pub_ip" ]] || printf '%s\n' "$local_ips" | tr ' ' '\n' | grep -qx "$r"; then
            matched="true"
            break
        fi
    done <<< "$resolved"

    local resolved_csv=""
    resolved_csv="$(printf '%s\n' "$resolved" | paste -sd ',' -)"

    if [[ "$matched" == "true" ]]; then
        ok "DNS A-запись '$fqdn' -> ${resolved_csv} совпадает с IP сервера"
        return 0
    fi

    warn "DNS A-запись '$fqdn' указывает на ${resolved_csv}, а IP этого сервера: ${pub_ip:-$local_ips}."
    return 1
}

prompt_dns_warn_continue() {
    local fqdn="$1"
    if ! check_dns_record "$fqdn"; then
        if [[ "$NON_INTERACTIVE" != "true" ]] && [[ -z "$ANSWERS_FD" ]]; then
            if ! prompt_confirm "DNS ещё не указывает на этот сервер (выпуск SSL может не пройти). Продолжить всё равно?" "Y"; then
                log "Установка остановлена пользователем. Обновите DNS A-запись и запустите скрипт повторно."
                exit 0
            fi
        fi
    fi
}

ensure_swap() {
    local mem_kb=0
    local swap_kb=0
    mem_kb="$(awk '/MemTotal/ {print $2}' /proc/meminfo)"
    swap_kb="$(awk '/SwapTotal/ {print $2}' /proc/meminfo)"
    local total_mb=$(( (mem_kb + swap_kb) / 1024 ))

    if [[ "$total_mb" -lt 1900 ]] && [[ ! -f /swapfile ]]; then
        log "ОЗУ+swap = ${total_mb}MB (<2G); создаю файл подкачки 2G (/swapfile)..."
        if fallocate -l 2G /swapfile 2>/dev/null || dd if=/dev/zero of=/swapfile bs=1M count=2048 status=none; then
            chmod 0600 /swapfile
            if mkswap /swapfile >/dev/null 2>&1 && swapon /swapfile 2>/dev/null; then
                if ! grep -q '^/swapfile ' /etc/fstab; then
                    printf '/swapfile none swap sw 0 0\n' >> /etc/fstab
                fi
            else
                warn "Не удалось активировать /swapfile (возможно, контейнер); продолжаем."
                rm -f /swapfile
            fi
        fi
    fi
}

configure_bbr() {
    if [[ "$ENABLE_BBR" != "true" ]]; then
        return 0
    fi
    log "Применяю TCP BBR + fq (/etc/sysctl.d/99-mypanel3.conf)..."
    cat > /etc/sysctl.d/99-mypanel3.conf <<'EOF'
net.core.default_qdisc = fq
net.ipv4.tcp_congestion_control = bbr
net.core.rmem_max = 16777216
net.core.wmem_max = 16777216
EOF
    sysctl --system >/dev/null 2>&1 || warn "sysctl --system вернул предупреждения (нормально для части контейнеров)"
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
        die "Порт ${port}/tcp уже занят другим процессом. Освободите порт и повторите запуск."
    fi
}

download_binary_from_panel_or_url() {
    local url="$1"
    local secret_hdr="${2:-}"
    local allow_fallback="${3:-false}"
    local tmp_dir=""
    tmp_dir="$(make_tmp_dir)"
    local bin_tmp="${tmp_dir}/mypanel3"
    local hdr_tmp="${tmp_dir}/headers.txt"

    log "Загрузка бинарника mypanel3 (${ARCH})..."
    local curl_args=(-fsSL -D "$hdr_tmp" -o "$bin_tmp")
    if [[ -n "$secret_hdr" ]]; then
        curl_args+=(-H "X-Node-Secret: ${secret_hdr}")
    fi

    if ! curl "${curl_args[@]}" "$url"; then
        if [[ "$allow_fallback" == "true" ]]; then
            warn "Не удалось скачать бинарник с $url"
            return 1
        fi
        die "Не удалось скачать бинарник mypanel3 с $url"
    fi

    if [[ ! -s "$bin_tmp" ]]; then
        if [[ "$allow_fallback" == "true" ]]; then
            warn "Скачанный файл с $url пуст"
            return 1
        fi
        die "Скачанный бинарник mypanel3 пуст"
    fi

    local expected_sha=""
    expected_sha="$(grep -i '^x-binary-sha256:' "$hdr_tmp" 2>/dev/null | head -n 1 | awk '{print $2}' | tr -d '\r\n' || true)"
    if [[ -n "$expected_sha" ]]; then
        local actual_sha=""
        actual_sha="$(sha256sum "$bin_tmp" | awk '{print $1}')"
        if [[ "$actual_sha" != "$expected_sha" ]]; then
            if [[ "$allow_fallback" == "true" ]]; then
                warn "Контрольная сумма SHA-256 не совпала ($expected_sha != $actual_sha)"
                return 1
            fi
            die "Ошибка проверки SHA-256 бинарника (ожидалось $expected_sha, получено $actual_sha)"
        fi
        ok "SHA-256 бинарника проверен (${expected_sha:0:16}...)."
    fi

    install -m 0755 "$bin_tmp" "$BIN_DEST"
    BINARY_PATH="$BIN_DEST"
    return 0
}

download_binary_from_github_release() {
    local asset_name="mypanel3-linux-${ARCH}"
    local base_url="https://github.com/${GITHUB_REPO}/releases/latest/download"
    local bin_url="${base_url}/${asset_name}"
    local sums_url="${base_url}/SHA256SUMS"
    local tmp_dir=""
    tmp_dir="$(make_tmp_dir)"
    local bin_tmp="${tmp_dir}/mypanel3"
    local sums_tmp="${tmp_dir}/SHA256SUMS"

    log "Загрузка ${asset_name} из GitHub Releases (${GITHUB_REPO})..."
    if ! curl -fsSL -o "$bin_tmp" "$bin_url" || [[ ! -s "$bin_tmp" ]]; then
        warn "Готовый бинарник ${asset_name} не найден в GitHub Releases (${bin_url})."
        return 1
    fi

    if curl -fsSL -o "$sums_tmp" "$sums_url" 2>/dev/null; then
        local expected_sha=""
        expected_sha="$(grep -E "[[:space:]](\./)?${asset_name}$" "$sums_tmp" 2>/dev/null | head -n 1 | awk '{print $1}' | tr -d '\r\n' || true)"
        if [[ -n "$expected_sha" ]]; then
            local actual_sha=""
            actual_sha="$(sha256sum "$bin_tmp" | awk '{print $1}')"
            if [[ "$actual_sha" != "$expected_sha" ]]; then
                warn "Ошибка SHA-256 для ${asset_name} (${expected_sha} != ${actual_sha})."
                return 1
            fi
            ok "SHA-256 релиза подтвержден (${expected_sha:0:16}...)."
        fi
    fi

    install -m 0755 "$bin_tmp" "$BIN_DEST"
    BINARY_PATH="$BIN_DEST"
    return 0
}

ensure_docker() {
    if ! command -v docker >/dev/null 2>&1; then
        log "Устанавливаю Docker Engine (для сборки из исходников)..."
        curl -fsSL https://get.docker.com | sh
    fi
    systemctl enable --now docker
}

build_binary_with_docker() {
    log "Сборка бинарника mypanel3 из исходников через Docker..."
    export DEBIAN_FRONTEND=noninteractive
    apt-get install -y -qq --no-install-recommends git
    ensure_docker

    local src_dir=""
    src_dir="$(make_tmp_dir)"
    local repo_url="https://github.com/${GITHUB_REPO}.git"
    log "Клонирую ${repo_url}..."
    git clone --depth 1 "$repo_url" "$src_dir" || die "Не удалось клонировать ${repo_url}"

    local image_tag="mypanel3-builder:install"
    log "Собираю Docker-образ (Rust + Node.js — это может занять несколько минут)..."
    docker build -t "$image_tag" "$src_dir" || die "Ошибка сборки Docker-образа mypanel3"

    local cid=""
    cid="$(docker create "$image_tag")"
    docker cp "${cid}:/usr/local/bin/mypanel3" "${src_dir}/mypanel3-built" || {
        docker rm -f "$cid" >/dev/null 2>&1 || true
        die "Не удалось извлечь бинарник mypanel3 из контейнера"
    }
    docker rm -f "$cid" >/dev/null 2>&1 || true
    docker image rm "$image_tag" >/dev/null 2>&1 || true

    install -m 0755 "${src_dir}/mypanel3-built" "$BIN_DEST"
    BINARY_PATH="$BIN_DEST"
    ok "Бинарник mypanel3 собран через Docker и установлен в $BIN_DEST"
}

resolve_binary() {
    if [[ -n "$BINARY_PATH" ]]; then
        [[ -x "$BINARY_PATH" ]] || die "Бинарник не найден или не исполняемый: $BINARY_PATH"
        return 0
    fi

    if [[ -n "$BINARY_URL" ]]; then
        download_binary_from_panel_or_url "$BINARY_URL" "$NODE_SECRET" "false"
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
        if download_binary_from_panel_or_url "$auto_url" "$NODE_SECRET" "true"; then
            return 0
        fi
        warn "Перехожу к загрузке бинарника с GitHub Releases..."
    fi

    if [[ "$UPGRADE_ONLY" != "true" ]] && [[ "$RECONFIGURE" != "true" ]] && [[ -x "$BIN_DEST" ]]; then
        BINARY_PATH="$BIN_DEST"
        return 0
    fi

    if download_binary_from_github_release; then
        return 0
    fi

    build_binary_with_docker
}

install_binary() {
    resolve_binary
    if [[ "$BINARY_PATH" != "$BIN_DEST" ]]; then
        log "Устанавливаю бинарник из $BINARY_PATH в $BIN_DEST"
        install -m 0755 "$BINARY_PATH" "$BIN_DEST"
    fi
}

install_base_packages() {
    log "Устанавливаю системные зависимости (curl, gnupg, ufw, dnsutils, jq и др.)..."
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends \
        ca-certificates curl gnupg lsb-release openssl unzip xz-utils tar dnsutils iproute2 jq ufw
}

install_postgres() {
    if ! command -v psql >/dev/null 2>&1; then
        log "Устанавливаю PostgreSQL..."
        export DEBIAN_FRONTEND=noninteractive
        apt-get install -y -qq --no-install-recommends postgresql postgresql-client
    fi
    systemctl enable --now postgresql
}

install_caddy() {
    if ! command -v caddy >/dev/null 2>&1; then
        log "Устанавливаю Caddy..."
        export DEBIAN_FRONTEND=noninteractive
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
        log "Устанавливаю HAProxy для SNI-мультиплексирования :443..."
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
        log "Устанавливаю Xray-core..."
        local xray_arch="64"
        [[ "$ARCH" == "arm64" ]] && xray_arch="arm64-v8a"
        local tmp_dir=""
        tmp_dir="$(make_tmp_dir)"
        if curl -fsSL "https://github.com/XTLS/Xray-core/releases/latest/download/Xray-linux-${xray_arch}.zip" -o "${tmp_dir}/xray.zip"; then
            unzip -q "${tmp_dir}/xray.zip" -d "${tmp_dir}/xray"
            install -m 0755 "${tmp_dir}/xray/xray" /usr/local/bin/xray
            [[ -f "${tmp_dir}/xray/geoip.dat" ]] && install -m 0644 "${tmp_dir}/xray/geoip.dat" /usr/local/share/xray/geoip.dat
            [[ -f "${tmp_dir}/xray/geosite.dat" ]] && install -m 0644 "${tmp_dir}/xray/geosite.dat" /usr/local/share/xray/geosite.dat
        else
            warn "Не удалось скачать Xray-core; при необходимости установите /usr/local/bin/xray вручную."
        fi
    fi

    if should_install_core "singbox" && [[ ! -x /usr/local/bin/sing-box ]]; then
        log "Устанавливаю sing-box..."
        local sb_ver=""
        sb_ver="$(curl -fsSL https://api.github.com/repos/SagerNet/sing-box/releases/latest | grep -oE '"tag_name":\s*"v[^"]+"' | head -n 1 | sed -E 's/.*"v([^"]+)".*/\1/' || true)"
        if [[ -n "$sb_ver" ]]; then
            local tmp_dir=""
            tmp_dir="$(make_tmp_dir)"
            local sb_tar="sing-box-${sb_ver}-linux-${ARCH}.tar.gz"
            if curl -fsSL "https://github.com/SagerNet/sing-box/releases/download/v${sb_ver}/${sb_tar}" -o "${tmp_dir}/sb.tar.gz"; then
                tar -xzf "${tmp_dir}/sb.tar.gz" -C "$tmp_dir"
                install -m 0755 "${tmp_dir}/sing-box-${sb_ver}-linux-${ARCH}/sing-box" /usr/local/bin/sing-box
            else
                warn "Не удалось скачать sing-box; при необходимости установите /usr/local/bin/sing-box вручную."
            fi
        else
            warn "Не удалось определить последнюю версию sing-box; пропускаю."
        fi
    fi

    if should_install_core "naive" && [[ ! -x /usr/local/bin/caddy-naive ]]; then
        if [[ "$ARCH" != "amd64" ]]; then
            warn "NaiveProxy не публикует готовые бинарники для linux/${ARCH}; пропускаю caddy-naive."
        else
            log "Устанавливаю NaiveProxy (caddy-forwardproxy)..."
            local naive_url=""
            naive_url="$(curl -fsSL https://api.github.com/repos/klzgrad/forwardproxy/releases/latest | grep -oE '"browser_download_url":\s*"[^"]+caddy[^"]*\.tar\.(xz|gz)"' | head -n 1 | sed -E 's/.*"(https:[^"]+)".*/\1/' || true)"
            if [[ -n "$naive_url" ]]; then
                local tmp_dir=""
                tmp_dir="$(make_tmp_dir)"
                if curl -fsSL "$naive_url" -o "${tmp_dir}/naive.tar.xz"; then
                    tar -xf "${tmp_dir}/naive.tar.xz" -C "$tmp_dir"
                    local naive_bin=""
                    naive_bin="$(find "$tmp_dir" -type f -name caddy | head -n 1 || true)"
                    if [[ -n "$naive_bin" ]]; then
                        install -m 0755 "$naive_bin" /usr/local/bin/caddy-naive
                    else
                        warn "Бинарник caddy не найден внутри архива NaiveProxy."
                    fi
                else
                    warn "Не удалось скачать архив NaiveProxy."
                fi
            else
                warn "Не удалось определить URL релиза NaiveProxy; пропускаю."
            fi
        fi
    fi

    if should_install_core "mieru" && [[ ! -x /usr/local/bin/mita ]]; then
        log "Устанавливаю Mieru server (mita)..."
        local mita_url=""
        mita_url="$(curl -fsSL https://api.github.com/repos/enfein/mieru/releases/latest | grep -oE "\"browser_download_url\":\s*\"https://[^\"]*mita_[^\"]*_linux_${ARCH}\\.tar\\.gz\"" | head -n 1 | sed -E 's/.*"(https:[^"]+)".*/\1/' || true)"
        if [[ -n "$mita_url" ]]; then
            local tmp_dir=""
            tmp_dir="$(make_tmp_dir)"
            if curl -fsSL "$mita_url" -o "${tmp_dir}/mita.tar.gz"; then
                tar -xzf "${tmp_dir}/mita.tar.gz" -C "$tmp_dir"
                local mita_bin=""
                mita_bin="$(find "$tmp_dir" -type f -name mita | head -n 1 || true)"
                if [[ -n "$mita_bin" ]]; then
                    install -m 0755 "$mita_bin" /usr/local/bin/mita
                else
                    warn "Бинарник mita не найден внутри архива Mieru."
                fi
            else
                warn "Не удалось скачать архив Mieru."
            fi
        else
            warn "Не удалось определить URL релиза Mieru; пропускаю."
        fi
    fi
}

detect_ssh_ports() {
    local ports=""
    if [[ -f /etc/ssh/sshd_config ]]; then
        ports="$(grep -hiE '^[[:space:]]*Port[[:space:]]+[0-9]+' /etc/ssh/sshd_config /etc/ssh/sshd_config.d/*.conf 2>/dev/null | awk '{print $2}' || true)"
    fi
    if command -v ss >/dev/null 2>&1; then
        local ss_ports=""
        ss_ports="$(ss -H -ltnp 2>/dev/null | awk '/sshd/ {print $4}' | sed -nE 's/.*:([0-9]+)$/\1/p' || true)"
        ports="${ports}"$'\n'"${ss_ports}"
    fi
    ports="$(printf '%s\n' "$ports" | grep -E '^[0-9]+$' | sort -nu || true)"
    if [[ -z "$ports" ]]; then
        ports="22"
    fi
    printf '%s' "$ports"
}

configure_firewall() {
    if [[ "$CONFIGURE_UFW" != "true" ]]; then
        UFW_STATUS="disabled (--no-ufw)"
        return 0
    fi
    if ! command -v ufw >/dev/null 2>&1; then
        warn "ufw не найден; пропускаю настройку фаервола."
        UFW_STATUS="unavailable"
        return 0
    fi

    log "Настраиваю UFW фаервол (SSH + порты режима ${MODE})..."
    local ssh_ports=""
    ssh_ports="$(detect_ssh_ports)"
    local sp=""
    while IFS= read -r sp; do
        [[ -n "$sp" ]] || continue
        ufw allow "${sp}/tcp" comment 'ssh' >/dev/null 2>&1 || true
    done <<< "$ssh_ports"

    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        if [[ "$PROXY_MODE" == "caddy" ]] && [[ -n "$DOMAIN" ]]; then
            ufw allow 80/tcp comment 'mypanel3-acme' >/dev/null 2>&1 || true
            ufw allow 443/tcp comment 'mypanel3-https' >/dev/null 2>&1 || true
            ufw allow 443/udp comment 'mypanel3-http3' >/dev/null 2>&1 || true
        else
            ufw allow 3000/tcp comment 'mypanel3-http' >/dev/null 2>&1 || true
        fi
    fi

    if [[ "$MODE" == "both" ]]; then
        ufw allow 8443/tcp comment 'mypanel3-naive-tcp' >/dev/null 2>&1 || true
        ufw allow 8443/udp comment 'mypanel3-naive-udp' >/dev/null 2>&1 || true
        ufw allow 2053/tcp comment 'mypanel3-mieru-tcp' >/dev/null 2>&1 || true
        ufw allow 2053/udp comment 'mypanel3-mieru-udp' >/dev/null 2>&1 || true
    fi

    if [[ "$MODE" == "node" ]]; then
        ufw allow 80/tcp comment 'mypanel3-node-acme' >/dev/null 2>&1 || true
        ufw allow 443/tcp comment 'mypanel3-node-proxy-tcp' >/dev/null 2>&1 || true
        ufw allow 443/udp comment 'mypanel3-node-proxy-udp' >/dev/null 2>&1 || true
        ufw allow 8443/tcp comment 'mypanel3-naive-tcp' >/dev/null 2>&1 || true
        ufw allow 8443/udp comment 'mypanel3-naive-udp' >/dev/null 2>&1 || true
        ufw allow 2053/tcp comment 'mypanel3-mieru-tcp' >/dev/null 2>&1 || true
        ufw allow 2053/udp comment 'mypanel3-mieru-udp' >/dev/null 2>&1 || true

        if [[ -n "$PANEL_IP" ]]; then
            local ip_entry=""
            IFS=',' read -ra _panel_entries <<< "$PANEL_IP"
            for ip_entry in "${_panel_entries[@]}"; do
                ip_entry="$(trim_ws "$ip_entry")"
                [[ -n "$ip_entry" ]] || continue
                ufw allow from "$ip_entry" to any port "$API_PORT" proto tcp comment 'mypanel3-panel' >/dev/null 2>&1 || true
            done
        else
            ufw allow "${API_PORT}/tcp" comment 'mypanel3-agent' >/dev/null 2>&1 || true
        fi
    fi

    if ufw --force enable >/dev/null 2>&1; then
        UFW_STATUS="active (SSH: $(printf '%s' "$ssh_ports" | paste -sd ',' -))"
        ok "UFW включен (${UFW_STATUS})."
    else
        warn "Не удалось включить UFW (возможно, недостаточно прав ядра в контейнере); продолжаю."
        UFW_STATUS="unavailable"
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
        local pub=""
        pub="$(detect_public_ipv4)"
        if [[ -n "$pub" ]]; then
            api_url="http://${pub}:3000"
            frontend_url="http://${pub}:3000"
        fi
        sub_url="$api_url"
        cors_origins="${api_url},http://127.0.0.1:3000"
    fi

    cat > "$PANEL_ENV" <<EOF
DATABASE_URL=${DATABASE_URL}
DB_MAX_CONNECTIONS=10
API_HOST=$( [[ "$PROXY_MODE" == "caddy" && -n "$DOMAIN" ]] && echo "127.0.0.1" || echo "0.0.0.0" )
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
    if [[ -z "$secret" ]] && [[ "$MODE" == "both" ]]; then
        secret="$(read_existing_env "$NODE_ENV_FILE" "NODE_RPC_SECRET")"
    fi
    if [[ -z "$secret" ]] && [[ "$RECONFIGURE" != "true" ]]; then
        secret="$(read_existing_env "$NODE_ENV_FILE" "NODE_RPC_SECRET")"
    fi
    if [[ -z "$secret" ]]; then
        secret="$(openssl rand -hex 24)"
    fi
    NODE_SECRET="$secret"

    local effective_master="$MASTER_URL"
    local effective_node_host="$NODE_DOMAIN"
    local effective_node_name="${NODE_NAME:-$(hostname -s 2>/dev/null || hostname)}"

    if [[ "$MODE" == "both" ]]; then
        effective_master="http://127.0.0.1:3000"
        AGENT_WORKER_HOST="0.0.0.0"
        AGENT_WORKER_PORT="$API_PORT"
        if [[ -z "$effective_node_host" ]]; then
            if [[ -n "$TLS_CERTS_DOMAIN" ]]; then
                effective_node_host="$TLS_CERTS_DOMAIN"
            elif [[ -n "$DOMAIN" ]]; then
                effective_node_host="$DOMAIN"
            else
                effective_node_host="$(detect_public_ipv4)"
                effective_node_host="${effective_node_host:-127.0.0.1}"
            fi
        fi
        if [[ -z "$PANEL_IP" ]]; then
            local pub=""
            pub="$(detect_public_ipv4)"
            local local_csv=""
            local_csv="$(hostname -I 2>/dev/null | tr ' ' '\n' | grep -E '^[0-9a-fA-F:.]+$' || true)"
            local combined_ips="127.0.0.1"$'\n'"::1"
            [[ -n "$pub" ]] && combined_ips="${combined_ips}"$'\n'"${pub}"
            [[ -n "$local_csv" ]] && combined_ips="${combined_ips}"$'\n'"${local_csv}"
            PANEL_IP="$(printf '%s\n' "$combined_ips" | awk 'NF && !seen[$0]++' | paste -sd ',' -)"
        fi
    else
        if [[ -z "$effective_node_host" ]]; then
            effective_node_host="$(detect_public_ipv4)"
            [[ -n "$effective_node_host" ]] || die "Не удалось определить адрес ноды; укажите --node-domain <DOMAIN|IP>"
        fi
        case "$TLS_MODE" in
            none)
                AGENT_WORKER_HOST="0.0.0.0"
                AGENT_WORKER_PORT="$API_PORT"
                ;;
            port2222|port2087)
                AGENT_WORKER_HOST="127.0.0.1"
                AGENT_WORKER_PORT="$((API_PORT + 1))"
                ;;
            auto|internal)
                AGENT_WORKER_HOST="127.0.0.1"
                AGENT_WORKER_PORT="$API_PORT"
                ;;
            *)
                die "Недопустимый --tls-mode '$TLS_MODE' (ожидается none, port2222, auto или internal)"
                ;;
        esac
    fi

    NODE_DOMAIN="$effective_node_host"
    NODE_NAME="$effective_node_name"

    cat > "$NODE_ENV_FILE" <<EOF
MASTER_URL=${effective_master}
NODE_RPC_SECRET=${secret}
NODE_SECRET=${secret}
NODE_HOST=${effective_node_host}
NODE_NAME=${effective_node_name}
WORKER_HOST=${AGENT_WORKER_HOST}
WORKER_PORT=${AGENT_WORKER_PORT}
ALLOWED_PANEL_IPS=${PANEL_IP}
PANEL_IP=${PANEL_IP}
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
    install_caddy
    install_haproxy
    export DEBIAN_FRONTEND=noninteractive
    apt-get install -y -qq --no-install-recommends certbot

    local pub_ip=""
    pub_ip="$(detect_public_ipv4)"
    [[ -n "$pub_ip" ]] || die "Не удалось определить публичный IPv4 для HAProxy :443 (используйте --no-sni-mux)"

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
            log "Выпуск сертификата Let's Encrypt для ${d}..."
            certbot certonly --webroot -w /var/www/acme --non-interactive --agree-tos "${email_args[@]}" -d "$d" \
                || die "Не удалось выпустить сертификат для ${d}. Проверьте DNS A-запись -> ${pub_ip} и открытый порт 80/tcp."
        fi
    done

    local inbound_tls="${TLS_CERTS_DOMAIN:-$DOMAIN}"
    is_ip_addr "$inbound_tls" && inbound_tls=""
    write_certbot_deploy_hook "${all_domains[*]}" "$inbound_tls"

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
        if [[ "$MODE" == "node" ]] && { [[ "$TLS_MODE" == "port2222" ]] || [[ "$TLS_MODE" == "port2087" ]]; }; then
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
        if [[ "$TLS_MODE" == "port2222" ]] || [[ "$TLS_MODE" == "port2087" ]]; then
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
        warn "Адрес '${TLS_CERTS_DOMAIN}' является IP; выпуск Let's Encrypt пропущен."
        return 0
    fi
    if [[ "$MODE" == "both" ]] && [[ "$SNI_MUX" == "true" ]]; then
        return 0
    fi

    if [[ "$TLS_CERTS_DOMAIN_EXPLICIT" != "true" ]] && ! check_dns_record "$TLS_CERTS_DOMAIN"; then
        warn "Пропускаю авто-выпуск Let's Encrypt для ${TLS_CERTS_DOMAIN}, так как DNS ещё не смотрит на этот сервер."
        warn "После обновления DNS выполните: certbot certonly --standalone -d ${TLS_CERTS_DOMAIN}"
        return 0
    fi

    log "Выпускаю сертификат Let's Encrypt для прокси-ядер (${TLS_CERTS_DOMAIN})..."
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
            || warn "Certbot не смог выпустить сертификат для ${TLS_CERTS_DOMAIN}; проверьте порт 80/tcp."
        if [[ "$stopped_caddy" == "true" ]]; then
            systemctl start caddy
        fi
    fi

    if [[ -f "/etc/letsencrypt/live/${TLS_CERTS_DOMAIN}/fullchain.pem" ]]; then
        write_certbot_deploy_hook "" "$TLS_CERTS_DOMAIN"
        ok "Сертификаты ноды сохранены в ${NODE_TLS_DIR}/"
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
    if [[ "$RECONFIGURE" != "true" ]] && [[ -f "$ADMIN_CREDS_FILE" ]] && [[ -z "$ADMIN_PASS" ]] && [[ "$GEN_ADMIN_PASS" != "true" ]]; then
        "$BIN_DEST" --env-file "$PANEL_ENV" admin create --username "$ADMIN_USER" --if-missing >/dev/null || true
        return 0
    fi

    local pass="$ADMIN_PASS"
    if [[ -z "$pass" ]]; then
        pass="$(openssl rand -base64 24 | tr -dc 'A-Za-z0-9!@#%^&*_-' | head -c 20)"
    fi

    local email_flag=()
    if [[ -n "$EMAIL" ]]; then
        email_flag=("--email" "$EMAIL")
    fi

    local extra_flags=("--if-missing")
    if [[ "$RECONFIGURE" == "true" ]]; then
        extra_flags=()
    fi

    "$BIN_DEST" --env-file "$PANEL_ENV" admin create \
        --username "$ADMIN_USER" \
        --password "$pass" \
        "${email_flag[@]}" \
        "${extra_flags[@]}"

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
    api_p="${api_p:-2222}"

    log "Регистрирую локальную ноду '${name}' (${host}, apiPort=${api_p}) в БД панели..."
    "$BIN_DEST" --env-file "$PANEL_ENV" node create \
        --name "$name" \
        --host "$host" \
        --port 443 \
        --api-port "$api_p" \
        --secret "$secret" \
        --if-missing
    LOCAL_NODE_STATUS="registered (host=${host}, port=${api_p})"
}

check_existing_installation() {
    if [[ -n "$MODE" ]] || [[ "$UPGRADE_ONLY" == "true" ]] || [[ "$RECONFIGURE" == "true" ]] || [[ "$NON_INTERACTIVE" == "true" ]] || [[ -n "$ANSWERS_FD" ]]; then
        return 0
    fi
    local has_panel="false"
    local has_node="false"
    [[ -f "$PANEL_ENV" ]] && has_panel="true"
    [[ -f "$NODE_ENV_FILE" ]] && has_node="true"

    if [[ "$has_panel" != "true" ]] && [[ "$has_node" != "true" ]]; then
        return 0
    fi

    local installed_desc=""
    if [[ "$has_panel" == "true" ]] && [[ "$has_node" == "true" ]]; then
        installed_desc="Панель + Нода"
    elif [[ "$has_panel" == "true" ]]; then
        installed_desc="Только Панель"
    else
        installed_desc="Только Нода"
    fi

    printf '%sОбнаружена существующая установка (%s):%s\n' "$C_YELLOW" "$installed_desc" "$C_RESET" >&2
    printf '  1) Обновить бинарник и перезапустить службы (конфиги и БД сохранятся)\n' >&2
    printf '  2) Переустановить / перезаписать конфигурацию\n' >&2
    printf '  3) Отменить\n' >&2
    local existing_choice=""
    existing_choice="$(prompt_line "Выберите действие (1-3)" "1")"
    case "$existing_choice" in
        1)
            UPGRADE_ONLY="true"
            run_upgrade
            exit 0
            ;;
        2)
            RECONFIGURE="true"
            ;;
        3)
            log "Установка отменена."
            exit 0
            ;;
        *)
            die "Неверный выбор: $existing_choice"
            ;;
    esac
}

run_wizard() {
    check_existing_installation

    if [[ -z "$MODE" ]]; then
        printf '%sВыберите сценарий установки:%s\n' "$C_BOLD" "$C_RESET" >&2
        printf '  1) Установить панель и локальную ноду на один сервер\n' >&2
        printf '  2) Установить только панель\n' >&2
        printf '  3) Установить только ноду\n' >&2
        printf '  4) Обновить существующую установку\n' >&2
        local choice=""
        choice="$(prompt_line "Ваш выбор (1-4)" "1")"
        case "$choice" in
            1|both) MODE="both" ;;
            2|panel) MODE="panel" ;;
            3|node) MODE="node" ;;
            4|upgrade)
                UPGRADE_ONLY="true"
                run_upgrade
                exit 0
                ;;
            *) die "Некорректный выбор сценария: $choice" ;;
        esac
    fi

    case "$MODE" in
        panel|node|both) ;;
        *) die "Недопустимый --mode '$MODE' (ожидается both, panel или node)" ;;
    esac

    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        if [[ "$PROXY_MODE" != "none" ]] && [[ -z "$DOMAIN" ]]; then
            DOMAIN="$(prompt_line "Домен панели (например, panel.example.com; Enter — без домена, HTTP :3000)" "")"
            if [[ -z "$DOMAIN" ]]; then
                PROXY_MODE="none"
            fi
        fi
        if [[ -n "$DOMAIN" ]]; then
            validate_domain "$DOMAIN" || die "Некорректный домен панели: $DOMAIN"
            if [[ "$PROXY_MODE" == "caddy" ]]; then
                prompt_dns_warn_continue "$DOMAIN"
            fi
        fi

        if [[ "$PROXY_MODE" != "none" ]] && [[ -z "$SUB_DOMAIN" ]] && [[ "$NON_INTERACTIVE" != "true" ]]; then
            SUB_DOMAIN="$(prompt_line "Отдельный домен для подписок (необязательно, Enter — общий с панелью)" "")"
        fi
        if [[ -n "$SUB_DOMAIN" ]]; then
            validate_domain "$SUB_DOMAIN" || die "Некорректный домен подписок: $SUB_DOMAIN"
            prompt_dns_warn_continue "$SUB_DOMAIN"
        fi

        if [[ "$PROXY_MODE" != "none" ]] && [[ "$TLS_MODE" == "auto" ]] && [[ -z "$EMAIL" ]]; then
            EMAIL="$(prompt_line "Email для Let's Encrypt и администратора (необязательно)" "")"
        fi

        if [[ "$ADMIN_USER_EXPLICIT" != "true" ]] && [[ "$NON_INTERACTIVE" != "true" ]] && { [[ "$RECONFIGURE" == "true" ]] || [[ ! -f "$ADMIN_CREDS_FILE" ]]; }; then
            ADMIN_USER="$(prompt_line "Логин администратора" "$ADMIN_USER")"
            [[ -n "$ADMIN_USER" ]] || die "Логин администратора не может быть пустым"
        fi

        if [[ -z "$ADMIN_PASS" ]] && [[ "$GEN_ADMIN_PASS" != "true" ]] && [[ "$NON_INTERACTIVE" != "true" ]] && { [[ "$RECONFIGURE" == "true" ]] || [[ ! -f "$ADMIN_CREDS_FILE" ]]; }; then
            local p1=""
            local p2=""
            p1="$(prompt_secret "Пароль администратора (мин. 12 символов, Enter — сгенерировать автоматически)")"
            if [[ -n "$p1" ]]; then
                [[ "${#p1}" -ge 12 ]] || die "Пароль администратора должен содержать не менее 12 символов"
                if [[ -z "$ANSWERS_FD" ]]; then
                    p2="$(prompt_secret "Повторите пароль администратора")"
                    [[ "$p1" == "$p2" ]] || die "Пароли не совпадают"
                fi
                ADMIN_PASS="$p1"
            else
                GEN_ADMIN_PASS="true"
            fi
        elif [[ -n "$ADMIN_PASS" ]] && [[ "${#ADMIN_PASS}" -lt 12 ]]; then
            die "Пароль администратора должен содержать не менее 12 символов"
        fi

        if [[ "$MODE" == "both" ]] && [[ -z "$NODE_DOMAIN" ]]; then
            local det_ip=""
            det_ip="$(detect_public_ipv4)"
            local default_node_dom="${DOMAIN:-$det_ip}"
            if [[ "$NON_INTERACTIVE" != "true" ]]; then
                NODE_DOMAIN="$(prompt_line "Домен или IP локальной ноды (может совпадать с доменом панели)" "$default_node_dom")"
            else
                NODE_DOMAIN="$default_node_dom"
            fi
        fi
        if [[ "$MODE" == "both" ]] && [[ -n "$NODE_DOMAIN" ]] && ! is_ip_addr "$NODE_DOMAIN"; then
            validate_domain "$NODE_DOMAIN" || die "Некорректный домен ноды: $NODE_DOMAIN"
            if [[ "$NODE_DOMAIN" != "$DOMAIN" ]] && [[ "$NODE_DOMAIN" != "$SUB_DOMAIN" ]]; then
                prompt_dns_warn_continue "$NODE_DOMAIN"
            fi
            if [[ "$TLS_CERTS_DOMAIN_EXPLICIT" != "true" ]] && [[ -z "$TLS_CERTS_DOMAIN" ]]; then
                TLS_CERTS_DOMAIN="$NODE_DOMAIN"
            fi
        fi
        if [[ "$MODE" == "both" ]] && [[ -z "$NODE_NAME" ]] && [[ "$NON_INTERACTIVE" != "true" ]]; then
            NODE_NAME="$(prompt_line "Имя локальной ноды" "$(hostname -s 2>/dev/null || hostname)")"
        fi
    fi

    if [[ "$MODE" == "node" ]]; then
        if [[ "$TLS_MODE_EXPLICIT" != "true" ]]; then
            TLS_MODE="none"
        fi

        if [[ -z "$NODE_DOMAIN" ]]; then
            local det_ip=""
            det_ip="$(detect_public_ipv4)"
            NODE_DOMAIN="$(prompt_line "Домен этой ноды (например, node1.example.com) или IP" "$det_ip")"
        fi
        [[ -n "$NODE_DOMAIN" ]] || die "Необходимо указать домен или IP ноды (--node-domain)"
        if ! is_ip_addr "$NODE_DOMAIN"; then
            validate_domain "$NODE_DOMAIN" || die "Некорректный домен ноды: $NODE_DOMAIN"
            prompt_dns_warn_continue "$NODE_DOMAIN"
            if [[ "$TLS_CERTS_DOMAIN_EXPLICIT" != "true" ]] && [[ -z "$TLS_CERTS_DOMAIN" ]]; then
                TLS_CERTS_DOMAIN="$NODE_DOMAIN"
            fi
        fi

        if [[ -z "$MASTER_URL" ]] && [[ -z "$PANEL_IP" ]]; then
            local panel_input=""
            panel_input="$(prompt_line "IP-адрес или домен управляющей панели (например, https://panel.example.com или 203.0.113.10)" "")"
            [[ -n "$panel_input" ]] || die "Необходимо указать IP или адрес управляющей панели"
            if is_ip_addr "$panel_input"; then
                PANEL_IP="$panel_input"
                if [[ "$NON_INTERACTIVE" != "true" ]]; then
                    local opt_url=""
                    opt_url="$(prompt_line "URL панели для обратного WSS (например, https://panel.example.com; Enter — только прямой порт ${API_PORT})" "")"
                    [[ -n "$opt_url" ]] && MASTER_URL="$(normalize_master_url "$opt_url")"
                fi
            else
                MASTER_URL="$(normalize_master_url "$panel_input")"
                local panel_host=""
                panel_host="$(extract_host_from_url "$MASTER_URL")"
                if [[ -n "$panel_host" ]] && ! is_ip_addr "$panel_host"; then
                    local resolved_panel_ips=""
                    resolved_panel_ips="$(resolve_domain_a "$panel_host" | paste -sd ',' - || true)"
                    if [[ "$NON_INTERACTIVE" != "true" ]]; then
                        if [[ -n "$resolved_panel_ips" ]]; then
                            local ip_ans=""
                            ip_ans="$(prompt_line "IP панели для ограничения доступа к порту ${API_PORT} ('any' — без привязки к IP)" "$resolved_panel_ips")"
                            if [[ "$ip_ans" != "any" ]] && [[ "$ip_ans" != "*" ]]; then
                                PANEL_IP="$ip_ans"
                            fi
                        else
                            PANEL_IP="$(prompt_line "IP панели для ограничения доступа к порту ${API_PORT} (Enter — принимать с любого IP по секрету)" "")"
                        fi
                    fi
                elif is_ip_addr "$panel_host"; then
                    PANEL_IP="$panel_host"
                fi
            fi
        elif [[ -n "$MASTER_URL" ]]; then
            local panel_host=""
            panel_host="$(extract_host_from_url "$MASTER_URL")"
            if [[ -z "$PANEL_IP" ]] && is_ip_addr "$panel_host"; then
                PANEL_IP="$panel_host"
            fi
        fi

        if [[ -z "$NODE_SECRET" ]]; then
            NODE_SECRET="$(prompt_secret "Секрет ноды (NODE_SECRET из панели, 16–256 символов)")"
        fi
        [[ -n "$NODE_SECRET" ]] || die "Необходимо указать секрет ноды (--node-secret)"
        validate_node_secret "$NODE_SECRET" || die "Секрет ноды должен быть длиной от 16 до 256 символов без пробелов"

        if [[ "$API_PORT_EXPLICIT" != "true" ]] && [[ "$NON_INTERACTIVE" != "true" ]]; then
            API_PORT="$(prompt_line "Порт управления нодой" "$API_PORT")"
        fi
        validate_port "$API_PORT" || die "Некорректный порт управления: $API_PORT"

        if [[ -z "$NODE_NAME" ]] && [[ "$NON_INTERACTIVE" != "true" ]]; then
            NODE_NAME="$(prompt_line "Имя ноды" "$(hostname -s 2>/dev/null || hostname)")"
        fi

        if is_ip_addr "$NODE_DOMAIN" && { [[ "$TLS_MODE" == "auto" ]] || [[ "$TLS_MODE" == "port2222" ]] || [[ "$TLS_MODE" == "port2087" ]]; }; then
            warn "Адрес ноды '${NODE_DOMAIN}' является IP; переключаю --tls-mode на 'none' (порт :${API_PORT}, :443 свободен)."
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
    step "Обновление mypanel3"
    ensure_curl
    install_binary
    if [[ -f "$PANEL_ENV" ]]; then
        log "Применяю миграции базы данных..."
        "$BIN_DEST" --env-file "$PANEL_ENV" migrate
    fi
    if systemctl list-unit-files | grep -q '^mypanel3\.service'; then
        systemctl restart mypanel3
        ok "Служба mypanel3 перезапущена"
    fi
    if systemctl list-unit-files | grep -q '^mypanel3-agent\.service'; then
        systemctl restart mypanel3-agent
        ok "Служба mypanel3-agent перезапущена"
    fi
    ok "Обновление завершено."
}

verify_installation() {
    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        log "Проверяю доступность панели (http://127.0.0.1:3000/api/health)..."
        local ready="false"
        for _ in $(seq 1 20); do
            if curl -fsS http://127.0.0.1:3000/api/health 2>/dev/null | grep -q '"ok"'; then
                ready="true"
                break
            fi
            sleep 1
        done
        if [[ "$ready" == "true" ]]; then
            PANEL_STATUS="ONLINE (active)"
            ok "Панель отвечает и готова к работе"
        else
            PANEL_STATUS="DEGRADED (check journalctl -u mypanel3)"
            warn "Панель не ответила за 20с (проверьте: journalctl -u mypanel3 -n 50)"
        fi
    fi

    if [[ "$MODE" == "node" ]] || [[ "$MODE" == "both" ]]; then
        local check_port="$AGENT_WORKER_PORT"
        local node_sec=""
        node_sec="$(read_existing_env "$NODE_ENV_FILE" "NODE_RPC_SECRET")"
        log "Проверяю агент ноды (http://127.0.0.1:${check_port}/api/status)..."
        local agent_ready="false"
        for _ in $(seq 1 15); do
            if curl -fsS \
                -H "Authorization: Bearer ${node_sec}" \
                -H "X-Node-Secret: ${node_sec}" \
                "http://127.0.0.1:${check_port}/api/status" >/dev/null 2>&1; then
                agent_ready="true"
                break
            fi
            sleep 1
        done
        if [[ "$agent_ready" == "true" ]]; then
            AGENT_STATUS="active (порт ${API_PORT} готов к приёму команд)"
            ok "Агент ноды активен и готов к приёму команд"
        else
            AGENT_STATUS="degraded (check journalctl -u mypanel3-agent)"
            warn "Агент ноды не ответил за 15с (проверьте: journalctl -u mypanel3-agent -n 50)"
        fi
    fi
}

print_summary() {
    local panel_url=""
    if [[ "$PROXY_MODE" == "caddy" ]] && [[ -n "$DOMAIN" ]]; then
        panel_url="https://${DOMAIN}"
    else
        local pub=""
        pub="$(detect_public_ipv4)"
        panel_url="http://${pub:-127.0.0.1}:3000"
    fi

    printf '\n%s%s' "$C_BOLD" "$C_GREEN"
    printf '╔══════════════════════════════════════════════════════════════════════╗\n'
    case "$MODE" in
        both)  printf '║  ✓ Установка завершена: Панель + Локальная Нода                      ║\n' ;;
        panel) printf '║  ✓ Установка завершена: Только Панель                                ║\n' ;;
        node)  printf '║  ✓ Установка завершена: Внешняя Нода                                 ║\n' ;;
    esac
    printf '╠══════════════════════════════════════════════════════════════════════╣%s\n' "$C_RESET"

    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        printf '%s║%s  %sURL панели:%s          %s%s%s\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$C_CYAN" "$panel_url" "$C_RESET"
        if [[ -n "$SUB_DOMAIN" ]] && [[ "$PROXY_MODE" == "caddy" ]]; then
            printf '%s║%s  %sURL подписок:%s        https://%s\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$SUB_DOMAIN"
        fi
        if [[ -f "$ADMIN_CREDS_FILE" ]]; then
            local u="" p=""
            u="$(grep '^username=' "$ADMIN_CREDS_FILE" | cut -d'=' -f2- || true)"
            p="$(grep '^password=' "$ADMIN_CREDS_FILE" | cut -d'=' -f2- || true)"
            printf '%s║%s  %sЛогин админа:%s        %s\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$u"
            printf '%s║%s  %sПароль админа:%s       %s%s%s\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$C_YELLOW" "$p" "$C_RESET"
        fi
        printf '%s║%s  %sСтатус панели:%s       %s\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$PANEL_STATUS"
        printf '%s║%s  %sКонфиг панели:%s       %s (учётные данные: %s)\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$PANEL_ENV" "$ADMIN_CREDS_FILE"
    fi

    if [[ "$MODE" == "node" ]] || [[ "$MODE" == "both" ]]; then
        local node_sec=""
        node_sec="$(read_existing_env "$NODE_ENV_FILE" "NODE_RPC_SECRET")"
        printf '%s║%s  %sСтатус службы ноды:%s  %s\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$AGENT_STATUS"
        printf '%s║%s  %sДомен / Хост ноды:%s   %s (%s)\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$NODE_DOMAIN" "$NODE_NAME"
        printf '%s║%s  %sПорт управления:%s     %s/tcp (tls-mode=%s)\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$API_PORT" "$TLS_MODE"
        if [[ -n "$PANEL_IP" ]]; then
            printf '%s║%s  %sРазрешённый IP панели:%s %s\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$PANEL_IP"
        fi
        if [[ -n "$MASTER_URL" ]] && [[ "$MODE" == "node" ]]; then
            printf '%s║%s  %sУправляющая панель:%s  %s (WSS /ws/worker)\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$MASTER_URL"
        fi
        if [[ "$MODE" == "both" ]]; then
            printf '%s║%s  %sЛокальная нода в БД:%s %s\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$LOCAL_NODE_STATUS"
            if [[ "$SNI_MUX" == "true" ]]; then
                printf '%s║%s  %sSNI Multiplexer:%s     HAProxy %s:443 -> Caddy :9443 + Xray :443\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "${PUBLIC_IP}"
            fi
        fi
        if [[ -f "${NODE_TLS_DIR}/fullchain.pem" ]]; then
            printf '%s║%s  %sTLS-сертификаты:%s     %s/{fullchain.pem,privkey.pem}\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$NODE_TLS_DIR"
        fi
        printf '%s║%s  %sПрокси-ядра:%s         ' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET"
        local b=""
        for b in xray sing-box caddy-naive mita; do
            if [[ -x "/usr/local/bin/${b}" ]]; then
                printf '%s[OK] ' "$b"
            else
                printf '%s[нет] ' "$b"
            fi
        done
        printf '\n'
        printf '%s║%s  %sКонфиг ноды:%s         %s\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$NODE_ENV_FILE"
        if [[ "$MODE" == "both" ]]; then
            printf '%s║%s  %sСекрет лок. ноды:%s    %s\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$node_sec"
        fi
    fi

    printf '%s║%s  %sФаервол UFW:%s         %s\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET" "$UFW_STATUS"

    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        printf '%s╠══════════════════════════════════════════════════════════════════════╣\n' "$C_GREEN"
        printf '║%s  %sПодключение внешней ноды:%s\n' "$C_RESET" "$C_BOLD" "$C_RESET"
        printf '%s║%s  1. Откройте %s%s%s -> Ноды -> Добавить ноду (порт %s2222%s).\n' "$C_GREEN" "$C_RESET" "$C_CYAN" "$panel_url" "$C_RESET" "$C_BOLD" "$C_RESET"
        printf '%s║%s  2. Скопируйте %sNODE_SECRET%s и на новом VPS выполните:\n' "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_RESET"
        printf '%s║%s     bash <(curl -fsSL https://raw.githubusercontent.com/%s/main/install.sh)\n' "$C_GREEN" "$C_RESET" "$GITHUB_REPO"
        printf '%s║%s     и выберите пункт 3 («Установить только ноду»).\n' "$C_GREEN" "$C_RESET"
    else
        printf '%s╠══════════════════════════════════════════════════════════════════════╣\n' "$C_GREEN"
        printf '║%s  %s✓ Нода запущена и готова принимать команды от панели на порту %s!%s\n' "$C_RESET" "$C_GREEN" "$API_PORT" "$C_RESET"
    fi

    printf '%s║%s  %sЛоги:%s                journalctl -u mypanel3 -f | journalctl -u mypanel3-agent -f\n' "$C_GREEN" "$C_RESET" "$C_DIM" "$C_RESET"
    printf '%s╚══════════════════════════════════════════════════════════════════════╝%s\n\n' "$C_GREEN" "$C_RESET"
}

main() {
    init_colors
    parse_args "$@"
    check_root
    check_os_and_arch
    ensure_curl
    detect_public_ipv4 >/dev/null || true
    print_banner

    if [[ "$UPGRADE_ONLY" == "true" ]]; then
        run_upgrade
        exit 0
    fi

    init_answers_fd
    run_wizard

    step "1/6 Подготовка системы и пакетов"
    ensure_swap
    install_base_packages
    configure_bbr

    step "2/6 Установка бинарника mypanel3"
    install_binary

    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        step "3/6 Проверка портов, PostgreSQL и миграции"
        check_port_free 5432 postgres
        check_port_free 3000 mypanel3
        if [[ "$PROXY_MODE" == "caddy" ]] && [[ -n "$DOMAIN" ]]; then
            check_port_free 80 caddy
            if [[ "$SNI_MUX" == "true" ]]; then
                check_port_free 9443 caddy
            fi
            check_port_free 443 caddy
        fi
        install_postgres
        ensure_panel_user_and_dirs
        setup_postgres_db
        write_panel_env
        log "Применяю миграции базы данных..."
        "$BIN_DEST" --env-file "$PANEL_ENV" migrate
        bootstrap_admin
    else
        step "3/6 Проверка конфигурации ноды"
    fi

    if [[ "$MODE" == "node" ]] || [[ "$MODE" == "both" ]]; then
        step "4/6 Установка прокси-ядер и конфигурации ноды"
        install_proxy_cores
        write_node_env
        check_port_free "$AGENT_WORKER_PORT" mypanel3
        if [[ "$MODE" == "node" ]]; then
            if [[ "$TLS_MODE" == "port2222" ]] || [[ "$TLS_MODE" == "port2087" ]]; then
                check_port_free 80 caddy
                check_port_free "$API_PORT" caddy
            elif [[ "$TLS_MODE" == "auto" ]] || [[ "$TLS_MODE" == "internal" ]]; then
                check_port_free 80 caddy
                check_port_free 443 caddy
            fi
        fi
    else
        step "4/6 Пропуск установки прокси-ядер (режим panel)"
    fi

    step "5/6 Настройка фаервола (UFW) и TLS / Reverse Proxy"
    configure_firewall

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

    step "6/6 Запуск служб systemd и проверка"
    register_local_node_in_db
    install_systemd_units

    if [[ "$MODE" == "panel" ]] || [[ "$MODE" == "both" ]]; then
        systemctl enable --now mypanel3
        systemctl restart mypanel3
    fi
    if [[ "$MODE" == "node" ]] || [[ "$MODE" == "both" ]]; then
        systemctl enable --now mypanel3-agent
        systemctl restart mypanel3-agent
    fi

    verify_installation
    print_summary
}

main "$@"
