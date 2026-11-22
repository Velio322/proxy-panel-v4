# Installing mypanel3 (ProxyPanel v4)

`install.sh` is an idempotent interactive installer for **Debian 12** (and 11+) and **Ubuntu 22.04–24.04** (and 20.04+) on `amd64` / `arm64`.
When run without `--binary` or `--binary-url`, fresh installs download the matching release binary from GitHub Releases (`Velio322/proxy-panel-v4`) verified against `SHA256SUMS` — and if no release binary matches, automatically install Docker/Git and build `mypanel3` from source. Re-running detects existing installs and preserves secrets, database passwords, and admin credentials unless `--reconfigure` is selected.

## Quick Start (GitHub One-Liner)

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh)
```

## Installation & Pairing Scenarios

| # | Scenario | Mode / Entrypoint | Key Features |
|---|---|---|---|
| **1** | **Panel + Node on one server** | `--mode both` | Panel + PostgreSQL + HAProxy SNI-Mux on `:443` + local agent on `:2222` auto-registered in DB + all 4 proxy cores + BBR + UFW |
| **2** | **Panel ONLY** | `--mode panel` | Lean control plane (Panel + PostgreSQL + Caddy HTTPS + optional `--sub-domain` + UFW `80/443/SSH`) |
| **3** | **Node ONLY (external VPS)** | `--mode node` | Agent on management port `2222` (IP-restricted via `ALLOWED_PANEL_IPS` + UFW + `NODE_SECRET`) + all 4 cores + BBR + `:443` kept free |
| **4** | **Attach node from Panel UI** | UI → *Nodes → Add Node* | Add node in panel on port `2222`; run generated command or paste `NODE_SECRET` into the interactive wizard |

---

## Scenario 1: Panel + Node on one server (All-in-One)

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh) \
  --mode both \
  --domain panel.example.com \
  --email you@example.com
```

What gets configured automatically:
- **PostgreSQL + Panel (`127.0.0.1:3000`)** + migrations + `admin` user in `/etc/mypanel3/admin-credentials.txt`
- **HAProxy SNI Multiplexer** on `<PUBLIC_IP>:443`:
  - `panel.example.com` (and optional `--sub-domain`) → Caddy (`127.0.0.1:9443`) → Panel
  - Everything else (e.g., Xray Reality SNI) → `127.0.0.1:443`
- **Certbot ACME webroot** (`/var/www/acme` on `:80`) + renewal hook copying certs into `/etc/caddy/certs/` and `/etc/mypanel3-node/tls/`
- **All 4 proxy cores** (`xray`, `sing-box`, `caddy-naive`, `mita`) + TCP BBR + UFW (`SSH`, `80`, `443`, `8443`, `2053`)
- **Local node auto-registered** in PostgreSQL on management port `2222` with auto-generated secret (`mypanel3 node create --if-missing`).

Pass `--no-sni-mux` to use Caddy directly on `:443`, or `--no-ufw` to skip firewall configuration.

---

## Scenario 2: Panel ONLY

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh) \
  --mode panel \
  --domain panel.example.com \
  --sub-domain sub.example.com \
  --email you@example.com
```

Without a domain (plain HTTP on `:3000`, e.g., behind your own reverse proxy):

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh) --mode panel --proxy none
```

---

## Scenario 3 & 4: External Node Setup

1. In your existing panel UI, open **Nodes → Add Node**, enter the new server's IP or domain (port `2222`), and save.
2. Copy the `NODE_SECRET` (or full 1-liner) and run on the target VPS (in interactive mode `install.sh` uses visible input for `NODE_SECRET`, automatically strips whitespace/newlines, validates length/format, and verifies port `2222`, UFW, and panel handshake before completion with an option to re-enter the token on mismatch):

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh) \
  --mode node \
  --master-url 'https://panel.example.com' \
  --panel-ip '<PANEL_IP>' \
  --node-domain 'node1.example.com' \
  --node-name 'eu-1' \
  --node-secret '<SECRET>' \
  --api-port 2222 \
  -y
```

When `--node-domain` is a DNS domain pointing to the node VPS, Let's Encrypt certificates are issued into `/etc/mypanel3-node/tls/` using port `80` while leaving `:443` free for Xray Reality / Sing-box.
If `--panel-ip` is set, UFW restricts port `2222/tcp` to that IP/CIDR and `mypanel3-agent` enforces `ALLOWED_PANEL_IPS` in addition to `NODE_SECRET` verification.

### Node Worker-API Transport Modes (`--tls-mode`)

| Mode | Port 443 | Agent API | Notes |
|---|---|---|---|
| `none` (default for `node`) | **FREE** | HTTP `:2222` on `0.0.0.0` + outbound TLS `WSS /ws/worker` | Control channel uses outbound WSS and/or panel→node API on `:2222` protected by `NODE_SECRET` + `ALLOWED_PANEL_IPS` |
| `port2222` | **FREE** | Caddy HTTPS on `:2222` → `127.0.0.1:2223` | HTTPS for inbound panel→node REST calls while keeping `:443` free (requires DNS domain & `:80` open) |
| `auto` | Caddy | Caddy Let's Encrypt on `:443` → `127.0.0.1:2222` | Legacy mode when proxy doesn't need `:443` |
| `internal` | Caddy | Self-signed Caddy CA on `:443` → `127.0.0.1:2222` | Testing only |

---

## Binary Distribution Sources

Priority order used by `install.sh`:
1. `--binary <PATH>` — local binary on disk.
2. `--binary-url <URL>` — explicit download URL.
3. Same-directory binary (`./mypanel3` or `./target/release/mypanel3`) when running from a source checkout or release tarball.
4. `--mode node` with `--master-url` + `--node-secret` — downloads directly from the master panel (`GET /api/v1/nodes/self/binary?arch=...` verified with `X-Binary-Sha256`; falls back on error).
5. Existing `/usr/local/bin/mypanel3` (skipped during `--upgrade` or `--reconfigure`).
6. GitHub Releases (`https://github.com/Velio322/proxy-panel-v4/releases/latest/download/mypanel3-linux-${ARCH}` verified against `SHA256SUMS`).
7. Automatic Docker source build (`git clone` + `docker build`) if no prebuilt binary is available.

---

## Upgrades, Reconfigure & Maintenance

```bash
# Upgrade binary from GitHub Releases + run DB migrations + restart services:
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh) --upgrade

# Re-run wizard to reconfigure an existing server:
sudo bash install.sh --reconfigure

# Uninstall (add --purge to remove configs & databases):
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/uninstall.sh)
```
