# ProxyPanel v4 (`mypanel3`)

Высокопроизводительная мультиядерная и мультипротокольная панель управления прокси-серверами с воркер-агентом в одном статическом Rust-бинарнике (`mypanel3`), встроенным React/Tailwind веб-интерфейсом и хранением состояния в PostgreSQL (без Redis).

---

## Быстрая установка (одной строкой)

Поддерживаются **Debian 12** (и Debian 11+) и **Ubuntu 22.04–24.04** (и 20.04+) на архитектурах `amd64` / `arm64`.
Скрипт не требует заранее установленных `git`, `docker`, `curl` или компиляторов: он сам проверяет систему, ставит зависимости, настраивает UFW, выпускает SSL-сертификаты Let's Encrypt и при отсутствии готового релиза собирает бинарник через Docker.

Запустите от `root` (или с `sudo`):

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh)
```

> При повторном запуске на уже настроенном сервере мастер предложит обновить бинарник с сохранением БД и ключей, перенастроить конфиг или отменить операцию.

---

## Сценарии развёртывания в интерактивном меню

После запуска откроется меню выбора из 3 сценариев (+ обновление):

### 1. Панель + локальная нода на одном сервере (`--mode both`)

Подходит для запуска всей инфраструктуры на одном VPS (требуется **2 домена**: отдельный для админ-панели `--domain` и отдельный для подписок `--sub-domain`):
- устанавливает PostgreSQL, `mypanel3` и Caddy + HAProxy SNI-мультиплексор на `:443` (HTTPS панели/подписок и Xray Reality работают на `:443` одновременно; на домене подписок открываются только `/sub/*` и `/api/v1/client/*`);
- выпускает SSL-сертификаты Let's Encrypt, настраивает UFW (`SSH`, `80`, `443`, `8443`, `2053`);
- устанавливает все 4 прокси-ядра (`xray`, `sing-box`, `caddy-naive`, `mita`), включает TCP BBR и автоматически регистрирует локальную ноду в БД с автогенерированным секретом на порту `2222`.

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh) \
  --mode both \
  --domain panel.example.com \
  --sub-domain sub.example.com \
  --email you@example.com
```

### 2. Только панель (`--mode panel`)

Лёгкий управляющий сервер (control plane): требует **2 домена** (`--domain` для админ-панели и `--sub-domain` для изолированной страницы подписок пользователей) + PostgreSQL + `mypanel3` + Caddy HTTPS + Let's Encrypt + UFW (`SSH`, `80`, `443`).

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh) \
  --mode panel \
  --domain panel.example.com \
  --sub-domain sub.example.com \
  --email you@example.com
```

### 3. Только нода — внешний сервер (`--mode node`)

Выделенный прокси-сервер для работы под управлением основной панели (требуется **1 домен** ноды):
- запрашивает домен ноды (`node1.example.com`) и сразу предлагает развернуть многостраничный **сайт-заглушку для Self-Steal Reality** (выбор из **10 готовых шаблонов** в [`decoys/`](decoys/) или случайный шаблон, раздаётся локальным Caddy на `127.0.0.1:9443` с Let's Encrypt, оставляя порт `:443` свободным под Xray);
- запрашивает адрес/IP управляющей панели и секрет `NODE_SECRET` (из **Ноды → Добавить ноду**, порт управления **`2222`**; ввод видимый с автоматической очисткой пробелов/переносов строк и валидацией);
- ограничивает доступ к управляющему порту `2222` как на уровне агента (`ALLOWED_PANEL_IPS`), так и правилом UFW (`allow from <PANEL_IP> to any port 2222`), проверяет секрет постоянным по времени сравнением (`X-Node-Secret` / `Authorization: Bearer`);
- после запуска агента проверяет прослушивание порта `2222`, правило UFW и рукопожатие с API панели (`NODE_SECRET`), позволяя при ошибке сразу ввести секрет заново без перезапуска установки.

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh) \
  --mode node \
  --master-url 'https://panel.example.com' \
  --panel-ip '<PANEL_IP>' \
  --node-domain 'node1.example.com' \
  --self-steal --decoy random \
  --node-name 'EU-1' \
  --node-secret '<NODE_SECRET>' \
  -y
```

### 4. Подключение ноды из веб-интерфейса панели

1. Откройте в панели **Ноды → Добавить ноду**, укажите IP/домен нового VPS и порт управления `2222`.
2. Скопируйте готовую однострочную команду (или только `NODE_SECRET` для интерактивного мастера) и выполните её на новом VPS. Панель раздаёт `/install.sh` и бинарник по `GET /api/v1/nodes/self/binary` с проверкой `X-Binary-Sha256`.

---

## Возможности панели

- **10 протоколов и 4 прокси-ядра:** VLESS (Reality, XHTTP, gRPC, WS, TCP, Post-Quantum ML-DSA-65, ECH, FinalMask), VMess, Trojan, Shadowsocks 2022, Hysteria2 (в т.ч. port hopping), NaiveProxy, Mieru, TUIC v5, AnyTLS и ShadowTLS v3 на базе `xray-core`, `sing-box`, `caddy-naive` и `mita`.
- **Гибкая привязка клиентов к нодам:** доступ ко всем нодам сразу или к выбранному списку серверов для каждого клиента (`ClientNode`), перерасчёт ревизии конфигурации и пуш на ноды.
- **Ссылки и QR-коды для всех протоколов:** готовые URI, конфиги и QR-коды (включая NaiveProxy и Mieru), подписки Base64, JSON, Clash / Mihomo, Sing-box, Happ (`crypt4`/`crypt5`), OpenWrt-экспорты, шаблоны, SRR-правила и контроль устройств (HWID).
- **Управление нодами и диагностика:** Xray Config Studio (редактор с живой валидацией), обновление версий прокси-ядер прямо из UI, поток логов, просмотр активных сессий, mTLS-сертификаты панель <-> нода, Prometheus-метрики `/metrics`.
- **Безопасность и эксплуатация:** 2FA (TOTP), Passkeys (WebAuthn), Telegram OAuth и админ-бот, автобэкапы (локально / Telegram / S3), тарифные планы, реселлеры, журнал аудита и биллинг серверов.

---

## Обновление и удаление

**Обновление до актуального релиза (`--upgrade`):**

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh) --upgrade
```

**Удаление служб и бинарника (добавьте `--purge` для удаления конфигов и данных):**

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/uninstall.sh)
```

---

## CLI-команды (`mypanel3`)

```text
mypanel3 [--env-file <PATH>] [--log-format text|json] <COMMAND>

Commands:
  serve         Run the master panel HTTP/WebSocket server (default)
  agent         Run the node worker agent daemon
  migrate       Run database migrations and exit
  admin create  Create an admin user (--username, --password, --email, --if-missing)
  node create   Register a node in DB (--name, --host, --port, --api-port, --secret, --if-missing)
  gen-secrets   Print fresh JWT_SECRET, ENCRYPTION_KEY, and METRICS_TOKEN values
```

---

## Docker Compose и сборка из исходников

Для запуска через Docker скопируйте `.env.example` в `.env`, задайте домены и секреты и выполните:

```bash
cp .env.example .env
docker compose up -d
docker compose exec mypanel3 mypanel3 admin create --username admin --if-missing
```

Сборка из исходников (требуются Rust >= 1.89 и Node.js >= 20):

```bash
cd client && npm ci && npm run build && cd ..
SQLX_OFFLINE=true cargo build --release --bin mypanel3
```

Подробнее обо всех флагах установщика, портах и режимах TLS — в [docs/install.md](docs/install.md).
