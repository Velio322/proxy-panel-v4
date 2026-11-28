import type { Protocol } from '@/types';

export type InboundModalTabId = 'general' | 'protocol' | 'security' | 'routing' | 'shares' | 'raw';

export type SecurityTabMode = 'xray' | 'hysteria2' | 'singbox-tls' | 'naive-tls' | 'none';

export interface ProtocolModalTab {
  id: InboundModalTabId;
  label: string;
}

export interface ProtocolModalConfig {
  description: string;
  tabs: ProtocolModalTab[];
  securityMode: SecurityTabMode;
  showSniffing: boolean;
  isXray: boolean;
  portLabel: string;
  portHint?: string;
  listenLabel: string;
  listenHint?: string;
}

export function getProtocolModalConfig(
  protocol: Protocol,
  isRu: boolean,
  portSharesCount: number,
): ProtocolModalConfig {
  const tGeneral = isRu ? 'Основные' : 'General';
  const tRouting = isRu ? 'Маршрутизация' : 'Routing';
  const tShares = `PortShares (${portSharesCount})`;
  const tRaw = 'Raw JSON';
  const defaultPortLabel = isRu ? 'Порт' : 'Port';
  const defaultListenLabel = isRu ? 'Адрес прослушивания' : 'Listen Address';

  switch (protocol) {
    case 'VLESS':
    case 'VMESS':
    case 'TROJAN':
      return {
        description: isRu
          ? `Настройка ${protocol}: транспорт Xray, шифрование TLS / Reality, маршрутизация и PortShares.`
          : `Configure ${protocol}: Xray transport, TLS / Reality security, routing, and PortShares.`,
        tabs: [
          { id: 'general', label: tGeneral },
          { id: 'protocol', label: isRu ? 'Протокол и транспорт' : 'Protocol & Transport' },
          { id: 'security', label: isRu ? 'Безопасность' : 'Security' },
          { id: 'routing', label: tRouting },
          { id: 'shares', label: tShares },
          { id: 'raw', label: tRaw },
        ],
        securityMode: 'xray',
        showSniffing: true,
        isXray: true,
        portLabel: defaultPortLabel,
        listenLabel: defaultListenLabel,
      };

    case 'SHADOWSOCKS':
      return {
        description: isRu
          ? 'Настройка Shadowsocks / SS-2022 на базе Xray, обёртка потока и маршрутизация.'
          : 'Configure Xray Shadowsocks / SS-2022 cipher, stream wrapper, and routing.',
        tabs: [
          { id: 'general', label: tGeneral },
          { id: 'protocol', label: 'Shadowsocks' },
          { id: 'security', label: isRu ? 'TLS и поток' : 'TLS & Stream' },
          { id: 'routing', label: tRouting },
          ...(portSharesCount > 0 ? [{ id: 'shares' as const, label: tShares }] : []),
          { id: 'raw', label: tRaw },
        ],
        securityMode: 'xray',
        showSniffing: true,
        isXray: true,
        portLabel: defaultPortLabel,
        listenLabel: defaultListenLabel,
      };

    case 'HYSTERIA2':
      return {
        description: isRu
          ? 'Настройка Hysteria2 (Sing-box, QUIC/UDP): полоса, Port Hopping, обфускация Salamander/Gecko и TLS.'
          : 'Configure Hysteria2 (Sing-box, QUIC/UDP): bandwidth limits, port hopping, Salamander/Gecko obfs, and TLS.',
        tabs: [
          { id: 'general', label: tGeneral },
          { id: 'protocol', label: 'Hysteria2 (QUIC)' },
          { id: 'security', label: isRu ? 'TLS и обфускация' : 'TLS & Obfuscation' },
          { id: 'routing', label: tRouting },
          { id: 'raw', label: tRaw },
        ],
        securityMode: 'hysteria2',
        showSniffing: false,
        isXray: false,
        portLabel: isRu ? 'UDP порт' : 'UDP Port',
        portHint: isRu
          ? 'Основной UDP порт (диапазон прыжков задаётся во вкладке Hysteria2)'
          : 'Primary UDP port (port hopping range is configured in Hysteria2 tab)',
        listenLabel: defaultListenLabel,
      };

    case 'TUIC':
      return {
        description: isRu
          ? 'Настройка TUIC v5 (Sing-box, QUIC/UDP): алгоритм перегрузки (BBR/CUBIC), 0-RTT и TLS.'
          : 'Configure TUIC v5 (Sing-box, QUIC/UDP): congestion control, 0-RTT handshake, and TLS.',
        tabs: [
          { id: 'general', label: tGeneral },
          { id: 'protocol', label: 'TUIC v5 (QUIC)' },
          { id: 'security', label: 'TLS' },
          { id: 'routing', label: tRouting },
          { id: 'raw', label: tRaw },
        ],
        securityMode: 'singbox-tls',
        showSniffing: false,
        isXray: false,
        portLabel: isRu ? 'UDP порт' : 'UDP Port',
        listenLabel: defaultListenLabel,
      };

    case 'ANYTLS':
      return {
        description: isRu
          ? 'Настройка AnyTLS (Sing-box, TCP/TLS): пул idle-сессий, схема паддинга и сертификат TLS.'
          : 'Configure AnyTLS (Sing-box, TCP/TLS): idle session pool, padding scheme, and TLS.',
        tabs: [
          { id: 'general', label: tGeneral },
          { id: 'protocol', label: isRu ? 'AnyTLS и паддинг' : 'AnyTLS & Padding' },
          { id: 'security', label: 'TLS' },
          { id: 'routing', label: tRouting },
          { id: 'raw', label: tRaw },
        ],
        securityMode: 'singbox-tls',
        showSniffing: false,
        isXray: false,
        portLabel: defaultPortLabel,
        listenLabel: defaultListenLabel,
      };

    case 'SHADOWTLS':
      return {
        description: isRu
          ? 'Настройка ShadowTLS v3 (Sing-box): рукопожатие с внешним сервером и переброс трафика (detour).'
          : 'Configure ShadowTLS v3 (Sing-box): external TLS handshake server and traffic detour.',
        tabs: [
          { id: 'general', label: tGeneral },
          { id: 'protocol', label: 'ShadowTLS Handshake' },
          { id: 'routing', label: tRouting },
          { id: 'raw', label: tRaw },
        ],
        securityMode: 'none',
        showSniffing: false,
        isXray: false,
        portLabel: defaultPortLabel,
        listenLabel: defaultListenLabel,
      };

    case 'NAIVEPROXY':
      return {
        description: isRu
          ? 'Настройка NaiveProxy (Caddy): сайт, режим HTTP/2 или HTTP/3, сайт-заглушка и сертификаты Caddy.'
          : 'Configure NaiveProxy (Caddy): site domain, HTTP/2 or HTTP/3 mode, decoy site, and Caddy TLS.',
        tabs: [
          { id: 'general', label: tGeneral },
          { id: 'protocol', label: 'NaiveProxy (Caddy)' },
          { id: 'security', label: 'TLS (Caddy)' },
          { id: 'raw', label: tRaw },
        ],
        securityMode: 'naive-tls',
        showSniffing: false,
        isXray: false,
        portLabel: defaultPortLabel,
        portHint: isRu
          ? 'Внешний HTTPS/QUIC порт Caddy (обычно 443 или 8443)'
          : 'Public Caddy HTTPS/QUIC port (usually 443 or 8443)',
        listenLabel: defaultListenLabel,
      };

    case 'MIERU':
      return {
        description: isRu
          ? 'Настройка Mieru (mita): транспорт TCP/UDP, одиночный порт или диапазон портов и MTU.'
          : 'Configure Mieru (mita): TCP/UDP transport, single port or port range, and MTU.',
        tabs: [
          { id: 'general', label: tGeneral },
          { id: 'protocol', label: 'Mieru (mita)' },
          { id: 'raw', label: tRaw },
        ],
        securityMode: 'none',
        showSniffing: false,
        isXray: false,
        portLabel: isRu ? 'Порт (базовый)' : 'Port (base)',
        portHint: isRu
          ? 'Игнорируется, если во вкладке Mieru задан диапазон портов'
          : 'Overridden when a Port Range is specified in the Mieru tab',
        listenLabel: defaultListenLabel,
      };

    case 'TGWEB':
      return {
        description: isRu
          ? 'Настройка Telegram WEB proxy (telemt): локальный loopback-порт, домен, carrier и FakeTLS.'
          : 'Configure Telegram WEB proxy (telemt): loopback listener port, public domain, carrier, and FakeTLS.',
        tabs: [
          { id: 'general', label: tGeneral },
          { id: 'protocol', label: 'Telegram WEB (telemt)' },
          { id: 'raw', label: tRaw },
        ],
        securityMode: 'none',
        showSniffing: false,
        isXray: false,
        portLabel: isRu ? 'Локальный порт (loopback)' : 'Loopback Port',
        portHint: isRu
          ? 'Внутренний порт telemt (например 18080); наружу 443 отдаёт Caddy'
          : 'Private telemt port (e.g. 18080); Caddy serves public 443',
        listenLabel: defaultListenLabel,
        listenHint: '127.0.0.1',
      };
  }
}
