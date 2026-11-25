#!/usr/bin/env bash
# Prepares a node for the Telegram WEB proxy (telemt): certificate + Caddy site on the private
# Self-Steal listener 127.0.0.1:9443 that forwards to telemt's loopback WEB port.
#
#   curl -fsSL https://<panel>/node-tgweb.sh | bash -s -- --domain tg.example.com
#
# Traffic path (Reality fallback): client -> :443 (xray Reality) -> unauthenticated TLS is passed
# to Reality's `dest` 127.0.0.1:9443 -> Caddy (real certificate) -> telemt 127.0.0.1:<port>.
# The script is idempotent: it replaces its own marked block and restores the previous Caddyfile
# if the new configuration does not validate.
set -euo pipefail

DOMAIN=""
LISTEN_PORT="18080"
BASE_PATH=""
EMAIL=""
CADDYFILE="${CADDYFILE:-/etc/caddy/Caddyfile}"
CERTS_DIR="${CERTS_DIR:-/etc/caddy/certs}"
ACME_ROOT="${ACME_ROOT:-/var/www/acme}"
BACKUP_DIR="${BACKUP_DIR:-/root/backups}"
DRY_RUN=0
MARK_BEGIN="# >>> mypanel3-tgweb (managed block, do not edit) >>>"
MARK_END="# <<< mypanel3-tgweb <<<"

log() { printf '[node-tgweb] %s\n' "$*"; }
die() { printf '[node-tgweb] ERROR: %s\n' "$*" >&2; exit 1; }

usage() {
    cat <<'EOF'
Usage: node-tgweb.sh --domain <dns name> [--port 18080] [--base-path telegram/web] [--email you@example.com] [--dry-run]
  --domain     DNS name of this node for the Telegram WEB proxy (A record must point to this node)
  --port       telemt loopback WEB port, must match the inbound port in the panel (default 18080)
  --base-path  serve telemt only under this path; the rest of the site shows the decoy
  --email      Let's Encrypt contact (optional)
  --dry-run    print the generated Caddy block and exit
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --domain) DOMAIN="${2:-}"; shift 2 ;;
        --port) LISTEN_PORT="${2:-}"; shift 2 ;;
        --base-path) BASE_PATH="${2:-}"; shift 2 ;;
        --email) EMAIL="${2:-}"; shift 2 ;;
        --dry-run) DRY_RUN=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done

[[ -n "$DOMAIN" ]] || { usage; die "--domain is required"; }
[[ "$DOMAIN" =~ ^([A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[A-Za-z]{2,}$ ]] \
    || die "invalid domain: $DOMAIN"
[[ "$DOMAIN" =~ ^[0-9.]+$ ]] && die "an IP address cannot be used, a DNS name is required"
[[ "$LISTEN_PORT" =~ ^[0-9]+$ ]] && (( LISTEN_PORT >= 1025 && LISTEN_PORT <= 65535 )) \
    || die "invalid --port: $LISTEN_PORT"
BASE_PATH="${BASE_PATH#/}"; BASE_PATH="${BASE_PATH%/}"
if [[ -n "$BASE_PATH" ]]; then
    [[ "$BASE_PATH" =~ ^[A-Za-z0-9][A-Za-z0-9_-]*(/[A-Za-z0-9][A-Za-z0-9_-]*)*$ ]] \
        || die "invalid --base-path: $BASE_PATH"
fi
if [[ -n "$EMAIL" && ! "$EMAIL" =~ ^[^[:space:]@]+@[^[:space:]@]+$ ]]; then die "invalid --email"; fi

CERT_FILE="${CERTS_DIR}/${DOMAIN}.fullchain.pem"
KEY_FILE="${CERTS_DIR}/${DOMAIN}.privkey.pem"

proxy_block() {
    cat <<EOF
reverse_proxy 127.0.0.1:${LISTEN_PORT} {
		header_up X-Forwarded-For {remote_host}
		flush_interval -1
		stream_close_delay 5m
	}
EOF
}

# `$1` = "acme" (HTTP-01 only, before the certificate exists) or "full".
render_block() {
    local stage="$1"
    printf '%s\n' "$MARK_BEGIN"
    cat <<EOF
http://${DOMAIN} {
	@acme path /.well-known/acme-challenge/*
	handle @acme {
		root * ${ACME_ROOT}
		file_server
	}
	handle {
		redir https://{host}{uri} 301
	}
}
EOF
    if [[ "$stage" == "full" ]]; then
        printf '\nhttps://%s:9443 {\n\tbind 127.0.0.1\n\ttls %s %s\n' "$DOMAIN" "$CERT_FILE" "$KEY_FILE"
        if [[ -n "$BASE_PATH" ]]; then
            printf '\thandle /%s/* {\n\t\t%s\n\t}\n\thandle {\n\t\trespond "Not Found" 404\n\t}\n' "$BASE_PATH" "$(proxy_block)"
        else
            printf '\t%s\n' "$(proxy_block)"
        fi
        printf '}\n'
    fi
    printf '%s\n' "$MARK_END"
}

if [[ $DRY_RUN -eq 1 ]]; then
    render_block full
    exit 0
fi

[[ $EUID -eq 0 ]] || die "run as root"
command -v caddy >/dev/null 2>&1 || die "caddy is not installed (run the node installer with --self-steal first)"
[[ -f "$CADDYFILE" ]] || die "$CADDYFILE not found"
grep -q "9443" "$CADDYFILE" || die "no Self-Steal listener on 127.0.0.1:9443 in $CADDYFILE; install the node with --self-steal first"
if grep -q "proxy_protocol" "$CADDYFILE"; then
    log "WARNING: $CADDYFILE uses proxy_protocol; make sure Reality 'xver' matches (client IPs)."
fi

install -d -m 0755 "$BACKUP_DIR"
STAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP="${BACKUP_DIR}/Caddyfile.tgweb.${STAMP}"
cp -a "$CADDYFILE" "$BACKUP"
log "backup: $BACKUP"

restore_and_die() {
    cp -a "$BACKUP" "$CADDYFILE"
    systemctl reload caddy 2>/dev/null || true
    die "$1 (previous Caddyfile restored)"
}

# Removes an earlier managed block, then appends the new one.
write_block() {
    local stage="$1" tmp
    tmp="$(mktemp)"
    awk -v b="$MARK_BEGIN" -v e="$MARK_END" '
        index($0, b) { skip = 1; next }
        skip && index($0, e) { skip = 0; next }
        !skip { print }
    ' "$CADDYFILE" > "$tmp"
    { cat "$tmp"; printf '\n'; render_block "$stage"; } > "$CADDYFILE"
    rm -f "$tmp"
    chmod 0644 "$CADDYFILE"
    caddy validate --config "$CADDYFILE" --adapter caddyfile >/dev/null 2>&1 \
        || restore_and_die "generated Caddyfile does not validate"
    systemctl reload caddy || restore_and_die "caddy reload failed"
}

install -d -m 0755 "$ACME_ROOT" "$CERTS_DIR"
chown -R caddy:caddy "$ACME_ROOT" 2>/dev/null || true
chown root:caddy "$CERTS_DIR" 2>/dev/null || true
chmod 0750 "$CERTS_DIR"

if [[ ! -s "$CERT_FILE" || ! -s "$KEY_FILE" ]]; then
    command -v certbot >/dev/null 2>&1 || {
        log "installing certbot"
        DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends certbot
    }
    log "publishing the ACME challenge route for ${DOMAIN}"
    write_block acme
    email_args=(--register-unsafely-without-email)
    [[ -n "$EMAIL" ]] && email_args=(--email "$EMAIL")
    if [[ ! -f "/etc/letsencrypt/live/${DOMAIN}/fullchain.pem" ]]; then
        certbot certonly --webroot -w "$ACME_ROOT" --non-interactive --agree-tos "${email_args[@]}" -d "$DOMAIN" \
            || restore_and_die "could not issue a certificate for ${DOMAIN}; check that its A record points to this node and port 80 is open"
    fi
    cp -L "/etc/letsencrypt/live/${DOMAIN}/fullchain.pem" "$CERT_FILE"
    cp -L "/etc/letsencrypt/live/${DOMAIN}/privkey.pem" "$KEY_FILE"
    chown root:caddy "$CERT_FILE" "$KEY_FILE"
    chmod 0640 "$CERT_FILE" "$KEY_FILE"

    hook="/etc/letsencrypt/renewal-hooks/deploy/mypanel3-tgweb-${DOMAIN}.sh"
    install -d -m 0755 "$(dirname "$hook")"
    cat > "$hook" <<EOF
#!/usr/bin/env bash
set -euo pipefail
cp -L "/etc/letsencrypt/live/${DOMAIN}/fullchain.pem" "${CERT_FILE}"
cp -L "/etc/letsencrypt/live/${DOMAIN}/privkey.pem" "${KEY_FILE}"
chown root:caddy "${CERT_FILE}" "${KEY_FILE}"
chmod 0640 "${CERT_FILE}" "${KEY_FILE}"
systemctl reload caddy || true
EOF
    chmod 0755 "$hook"
fi

write_block full
log "Caddy now serves ${DOMAIN} on 127.0.0.1:9443 -> telemt 127.0.0.1:${LISTEN_PORT}"

if command -v openssl >/dev/null 2>&1; then
    subject="$(echo | timeout 8 openssl s_client -connect 127.0.0.1:443 -servername "$DOMAIN" 2>/dev/null \
        | openssl x509 -noout -subject 2>/dev/null || true)"
    if [[ "$subject" == *"$DOMAIN"* ]]; then
        log "OK: TLS on :443 with SNI ${DOMAIN} returns its certificate (${subject})"
    else
        log "WARNING: :443 with SNI ${DOMAIN} did not return the certificate for ${DOMAIN}."
        log "         Check that Reality 'dest' is 127.0.0.1:9443 (got: ${subject:-nothing})."
    fi
fi
log "Next: create a 'Telegram WEB' inbound in the panel (port ${LISTEN_PORT}, domain ${DOMAIN}, publicAddr <node IP>:443)."
