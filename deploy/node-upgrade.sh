#!/usr/bin/env bash
# Upgrades the mypanel3 binary on a node in place, from the panel that manages it.
#
#   sudo bash node-upgrade.sh [panel-base-url]
#
# - Reads MASTER_URL and NODE_RPC_SECRET from /etc/mypanel3/node.env, so no secret has to be
#   typed or pasted. An explicit base URL (https://panel.example.com) overrides MASTER_URL.
# - Downloads /api/v1/nodes/self/binary, verifies ELF magic, the X-Binary-Sha256 header and
#   `--version`, keeps a backup of the running binary, swaps it atomically, restarts the agent
#   and rolls back if the service does not stay up.
# - Does nothing when the node already runs the panel's binary.
set -euo pipefail

ENV_FILE="${ENV_FILE:-/etc/mypanel3/node.env}"
BIN_DEST="${BIN_DEST:-/usr/local/bin/mypanel3}"
SERVICE="${SERVICE:-mypanel3-agent}"
BACKUP_DIR="${BACKUP_DIR:-/root/backups}"
STABLE_SECS="${STABLE_SECS:-20}"

log() { printf '[node-upgrade] %s\n' "$*"; }
die() { printf '[node-upgrade] ERROR: %s\n' "$*" >&2; exit 1; }

[[ $EUID -eq 0 ]] || die "run as root"
[[ -r "$ENV_FILE" ]] || die "cannot read $ENV_FILE (is this a node?)"
[[ -x "$BIN_DEST" ]] || die "$BIN_DEST not found"

env_get() {
    grep -E "^$1=" "$ENV_FILE" | tail -n 1 | cut -d= -f2- \
        | sed -E "s/^['\"]//; s/['\"]\$//" | tr -d '\r'
}

secret="$(env_get NODE_RPC_SECRET)"
[[ -n "$secret" ]] || die "NODE_RPC_SECRET missing in $ENV_FILE"

base="${1:-$(env_get MASTER_URL)}"
[[ -n "$base" ]] || die "MASTER_URL missing in $ENV_FILE and no URL argument given"
base="$(printf '%s' "$base" | sed -E 's#^wss://#https://#; s#^ws://#http://#; s#/ws/worker.*$##; s#/+$##')"
case "$base" in
    https://*|http://127.0.0.1|http://127.0.0.1:*|http://127.0.0.1/*|http://localhost|http://localhost:*|http://localhost/*) ;;
    *) die "refusing non-HTTPS panel URL: $base" ;;
esac

case "$(uname -m)" in
    x86_64|amd64) arch=amd64 ;;
    aarch64|arm64) arch=arm64 ;;
    *) die "unsupported architecture $(uname -m)" ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

log "downloading $base/api/v1/nodes/self/binary (arch=$arch)"
escaped_secret="${secret//\\/\\\\}"
escaped_secret="${escaped_secret//\"/\\\"}"
printf 'header = "X-Node-Secret: %s"\n' "$escaped_secret" | curl -fsS -K - --max-time 300 \
    -D "$tmp/headers" -o "$tmp/new" "$base/api/v1/nodes/self/binary?arch=$arch" \
    || die "download failed (wrong panel URL or secret?)"

[[ -s "$tmp/new" ]] || die "downloaded file is empty"
[[ "$(head -c 4 "$tmp/new" | od -An -tx1 | tr -d ' \n')" == "7f454c46" ]] || die "downloaded file is not an ELF binary"

expected="$(grep -i '^x-binary-sha256:' "$tmp/headers" | head -n 1 | awk '{print $2}' | tr -d '\r\n' || true)"
[[ "$expected" =~ ^[0-9a-fA-F]{64}$ ]] || die "missing or invalid X-Binary-Sha256 header"
actual="$(sha256sum "$tmp/new" | awk '{print $1}')"
[[ "${actual,,}" == "${expected,,}" ]] || die "SHA-256 mismatch (expected $expected, got $actual)"

chmod 0755 "$tmp/new"
version="$("$tmp/new" --version 2>&1 | head -n 1)" || die "new binary does not run"
log "new binary verified: $version (sha256 ${actual:0:16}...)"

current="$(sha256sum "$BIN_DEST" | awk '{print $1}')"
if [[ "$current" == "$actual" ]]; then
    log "already up to date, nothing to do"
    exit 0
fi

mkdir -p "$BACKUP_DIR"
chmod 700 "$BACKUP_DIR"
backup="$BACKUP_DIR/mypanel3-node-$(date +%Y%m%d-%H%M%S).bak"
cp -a "$BIN_DEST" "$backup"
log "backup: $backup"

rollback() {
    log "rolling back to $backup"
    install -m 0755 "$backup" "$BIN_DEST.rollback"
    mv -f "$BIN_DEST.rollback" "$BIN_DEST"
    systemctl restart "$SERVICE" || true
}

install -m 0755 "$tmp/new" "$BIN_DEST.new"
mv -f "$BIN_DEST.new" "$BIN_DEST"

log "restarting $SERVICE"
if ! systemctl restart "$SERVICE"; then
    rollback
    die "restart failed, previous binary restored"
fi

sleep 3
pid="$(systemctl show -p MainPID --value "$SERVICE")"
for ((i = 0; i < STABLE_SECS; i++)); do
    now_pid="$(systemctl show -p MainPID --value "$SERVICE")"
    if ! systemctl is-active --quiet "$SERVICE" || [[ -z "$now_pid" || "$now_pid" == "0" || "$now_pid" != "$pid" ]]; then
        rollback
        die "$SERVICE did not stay up, previous binary restored (see: journalctl -u $SERVICE -n 50)"
    fi
    sleep 1
done

log "done: $SERVICE is running $version (backup kept at $backup)"
