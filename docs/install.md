# Installing mypanel3 (ProxyPanel v4)

`install.sh` is an idempotent installer for Debian 11+ / Ubuntu 20.04+ (`amd64` / `arm64`).
When run without `--binary` or `--binary-url`, fresh panel/both installs and `--upgrade` runs automatically download the matching release binary from GitHub Releases (`Velio322/proxy-panel-v4`) and verify it against `SHA256SUMS`. Re-running keeps existing secrets, database passwords, and admin credentials intact.

## Quick Start (GitHub One-Liner)

```bash
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh | sudo bash
```

## 4 Installation & Pairing Scenarios

| # | Scenario | Mode / Entrypoint | Key Features |
|---|---|---|---|
| **1** | **Panel + Node on one server** | `--mode both` | Panel + PostgreSQL + HAProxy SNI-Mux on `:443` (Panel HTTPS & Xray Reality share `:443`) + local agent auto-registered in DB + all 4 proxy cores + BBR |
| **2** | **Panel ONLY** | `--mode panel` | Lean control plane (Panel + PostgreSQL + Caddy HTTPS + optional `--sub-domain`) without local proxy cores/agent |
| **3** | **Node ONLY (dedicated VPS)** | `--mode node` | Agent + all 4 cores (`xray`, `sing-box`, `caddy-naive`, `mita`) + BBR; **keeps port `:443` 100% free** for Reality |
| **4** | **Connect new node to existing panel** | UI -> *Nodes -> Add Node* -> 1-liner | Add node in panel even before agent exists; run generated command on new VPS - `install.sh` pulls `mypanel3` from your panel (SHA-256 verified) & pairs via WSS |

---

## Scenario 1: Panel + Node on one server (All-in-One)

```bash
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh | sudo bash -s -- \
  --mode both \
  --domain panel.example.com \
  --email you@example.com
```

What gets configured automatically:
- **PostgreSQL + Panel (`127.0.0.1:3000`)** + migrations + `admin` user in `/etc/mypanel3/admin-credentials.txt`
- **HAProxy SNI Multiplexer** on `<PUBLIC_IP>:443`:
  - `panel.example.com` (and optional `--sub-domain`) -> Caddy (`127.0.0.1:9443`) -> Panel
  - Everything else (e.g., Xray Reality SNI) -> `127.0.0.1:443` (VLESS Reality shares `:443` cleanly with the HTTPS panel)
- **Certbot ACME webroot** (`/var/www/acme` on `:80`) + renewal hook copying certs into `/etc/caddy/certs/` and `/etc/mypanel3-node/tls/`
- **All 4 proxy cores** (`xray`, `sing-box`, `caddy-naive`, `mita`) + TCP BBR tuning
- **Local node auto-registered** in PostgreSQL (`mypanel3 node create --if-missing`), immediately coming `ONLINE`.

To skip HAProxy SNI-mux (use Caddy directly on `:443`): pass `--no-sni-mux`.

---

## Scenario 2: Panel ONLY

```bash
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh | sudo bash -s -- \
  --mode panel \
  --domain panel.example.com \
  --sub-domain sub.example.com \
  --email you@example.com
```

Without a domain (plain HTTP on `:3000`, e.g., behind your own reverse proxy):

```bash
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh | sudo bash -s -- --mode panel --proxy none
```

---

## Scenario 3 & 4: New Node / Connect Node to Existing Panel

1. In your existing panel UI, open **Nodes -> Add Node**, enter the new server's IP or domain (no running agent required beforehand), and save.
2. Copy the generated 1-line command shown in the **Node Deployment & Pairing** modal, and run it on the new VPS as `root`:

```bash
curl -fsSL https://panel.example.com/install.sh | sudo bash -s -- \
  --mode node \
  --master-url 'https://panel.example.com' \
  --node-name 'eu-1' \
  --node-domain 'node1.example.com' \
  --node-secret '<secret>' \
  --api-port 2087 \
  --tls-mode none -y
```

Add `--tls-certs-domain 'node1.example.com'` to issue Let's Encrypt certificates into `/etc/mypanel3-node/tls/` for NaiveProxy / Hysteria2 / Trojan (using port `80`, leaving `:443` free for Xray Reality).

### Node Worker-API Transport Modes (`--tls-mode`)

| Mode | Port 443 | Agent API | Notes |
|---|---|---|---|
| `none` (default for `node`) | **FREE** | HTTP `:2087` on `0.0.0.0` + outbound TLS `WSS /ws/worker` | Control channel uses outbound WSS over TLS to master panel; works behind NAT/firewalls too |
| `port2087` | **FREE** | Caddy HTTPS on `:2087` -> `127.0.0.1:2088` | HTTPS for inbound panel->node REST calls while keeping `:443` free (requires DNS domain & `:80` open) |
| `auto` | Caddy | Caddy Let's Encrypt on `:443` -> `127.0.0.1:2087` | Legacy mode when proxy doesn't need `:443` |
| `internal` | Caddy | Self-signed Caddy CA on `:443` -> `127.0.0.1:2087` | Testing only |

---

## Binary Distribution Sources

Priority order used by `install.sh`:
1. `--binary <PATH>` — local binary on disk.
2. `--binary-url <URL>` — explicit download URL.
3. Same-directory binary (`./mypanel3` or `./target/release/mypanel3`) when running from a source checkout or release tarball.
4. `--mode node` with `--master-url` + `--node-secret` — downloads directly from the master panel (`GET /api/v1/nodes/self/binary?arch=...` verified with `X-Binary-Sha256`).
5. Existing `/usr/local/bin/mypanel3` (skipped during `--upgrade`).
6. GitHub Releases (`https://github.com/Velio322/proxy-panel-v4/releases/latest/download/mypanel3-linux-${ARCH}` verified against `SHA256SUMS`).

> **Cross-architecture nodes (e.g., `arm64` node connecting to an `amd64` panel):** place a prebuilt `mypanel3-linux-arm64` binary at `/var/lib/mypanel3/binaries/mypanel3-linux-arm64` on the panel host, or pass `--binary-url <url>`.

---

## Upgrades & Maintenance

```bash
# Upgrade binary from GitHub Releases + run DB migrations + restart services:
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh | sudo bash -s -- --upgrade

# Or upgrade from a local binary:
sudo ./install.sh --upgrade --binary /path/to/new/mypanel3

# Interactive wizard (all 4 options):
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh | sudo bash

# Uninstall (add --purge to remove configs & databases):
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/uninstall.sh | sudo bash
```
