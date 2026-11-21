# ProxyPanel v4 (`mypanel3`)

Высокопроизводительная мультиядерная и мультипротокольная панель управления прокси-серверами с воркер-агентом в одном статическом Rust-бинарнике (`mypanel3`), встроенным React/Tailwind веб-интерфейсом и хранением состояния в PostgreSQL (без Redis).

---

## Быстрая установка (одной строкой)

Поддерживаются **Debian 11+** и **Ubuntu 20.04+** (`amd64`; для `arm64` укажите `--binary` или `--binary-url`).
Скрипт автоматически скачивает актуальный бинарник из GitHub Releases, проверяет контрольную сумму `SHA256SUMS` и запускает интерактивный мастер выбора режима:

```bash
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh | sudo bash
```

---

## 4 сценария развёртывания

### 1. Панель + Нода на одном сервере (`--mode both`)

Подходит для запуска всей инфраструктуры на одном VPS:
- устанавливает PostgreSQL, `mypanel3` и Caddy;
- поднимает HAProxy SNI-мультиплексор на `:443` (HTTPS панели и Xray Reality делят порт `:443` без конфликтов);
- устанавливает все 4 прокси-ядра (`xray`, `sing-box`, `caddy-naive`, `mita`), включает BBR и автоматически регистрирует локальную ноду в БД.

```bash
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh | sudo bash -s -- \
  --mode both \
  --domain panel.example.com \
  --email you@example.com
```

### 2. Только панель (`--mode panel`)

Лёгкий управляющий сервер (control plane): PostgreSQL + `mypanel3` + Caddy HTTPS (опционально отдельный `--sub-domain`). Прокси-ядра, агент и HAProxy не устанавливаются.

```bash
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh | sudo bash -s -- \
  --mode panel \
  --domain panel.example.com \
  --sub-domain sub.example.com \
  --email you@example.com
```

### 3. Только нода (`--mode node`)

Выделенный прокси-сервер для работы под управлением основной панели:
- режим по умолчанию `--tls-mode none` оставляет порт `:443` полностью свободным под VLESS Reality (и другие инбаунды);
- работает как с доменом, так и с голым IP;
- воркер-агент сам держит защищённое соединение `WSS /ws/worker` к мастер-панели.

```bash
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh | sudo bash -s -- \
  --mode node \
  --master-url 'https://panel.example.com' \
  --node-domain 'node1.example.com' \
  --node-name 'EU-1' \
  --node-secret '<SECRET>' \
  -y
```

### 4. Подключение новой ноды напрямую к существующей панели

1. Откройте в панели **Ноды -> Добавить ноду**, введите хост/IP будущего VPS (заранее установленный агент не нужен — панель сохраняет ноду сразу) и скопируйте сгенерированную однострочную команду.
2. Выполните команду на новом сервере:

```bash
curl -fsSL https://panel.example.com/install.sh | sudo bash -s -- \
  --mode node \
  --master-url 'https://panel.example.com' \
  --node-domain 'node1.example.com' \
  --node-name 'EU-1' \
  --node-secret '<SECRET>'
```

Панель сама раздаёт скрипт `/install.sh` и бинарник с `GET /api/v1/nodes/self/binary` (авторизация по `X-Node-Secret`, проверка `X-Binary-Sha256`), поэтому на новой ноде не нужны Git, Rust или Node.js.

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
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh | sudo bash -s -- --upgrade
```

**Удаление служб и бинарника (добавьте `--purge` для удаления конфигов и данных):**

```bash
curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/uninstall.sh | sudo bash
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

Подробнее обо всех флагах установщика и режимах TLS — в [docs/install.md](docs/install.md).
