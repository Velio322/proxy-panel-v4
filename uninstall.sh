#!/usr/bin/env bash
set -euo pipefail

# mypanel3 uninstaller

PURGE="false"
ASSUME_YES="false"

log() {
    printf '[mypanel3] %s\n' "$*"
}

die() {
    printf '[mypanel3] ERROR: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Usage: sudo ./uninstall.sh [OPTIONS]

Options:
  --purge               Also remove /etc/mypanel3, /var/lib/mypanel3, /var/lib/mypanel3-node, /etc/mypanel3-node
  -y, --yes, --non-interactive
                        Do not prompt for confirmation
  -h, --help            Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --purge)
            PURGE="true"
            shift
            ;;
        -y|--yes|--non-interactive)
            ASSUME_YES="true"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            die "Unknown option: $1"
            ;;
    esac
done

if [[ "$(id -u)" -ne 0 ]]; then
    die "Please run as root (e.g. sudo ./uninstall.sh)"
fi

if [[ "$PURGE" == "true" ]] && [[ "$ASSUME_YES" != "true" ]]; then
    printf 'This will remove mypanel3 binaries, config, and data directories. Continue? [y/N]: '
    IFS= read -r reply || reply=""
    case "$reply" in
        y|Y|yes|YES) ;;
        *)
            log "Aborted."
            exit 0
            ;;
    esac
fi

for svc in mypanel3 mypanel3-agent; do
    if systemctl list-unit-files | grep -q "^${svc}\.service"; then
        log "Stopping and disabling ${svc}..."
        systemctl disable --now "$svc" 2>/dev/null || true
        rm -f "/etc/systemd/system/${svc}.service"
    fi
done

systemctl daemon-reload 2>/dev/null || true

if [[ -f /usr/local/bin/mypanel3 ]]; then
    log "Removing /usr/local/bin/mypanel3..."
    rm -f /usr/local/bin/mypanel3
fi

if [[ "$PURGE" == "true" ]]; then
    log "Purging configuration and data directories..."
    rm -rf /etc/mypanel3 /var/lib/mypanel3 /var/lib/mypanel3-node /etc/mypanel3-node
    if id -u mypanel3 >/dev/null 2>&1; then
        userdel mypanel3 2>/dev/null || true
    fi
fi

log "Uninstall complete."
