import { PROTOCOL_LABELS, getCoreForProtocol } from '@/lib/core-utils';
import type { Inbound, Protocol } from '@/types';

export type ProtocolIconKey =
  | 'vless'
  | 'reality'
  | 'vmess'
  | 'trojan'
  | 'shadowsocks'
  | 'hysteria2'
  | 'naiveproxy'
  | 'mieru'
  | 'tuic'
  | 'anytls'
  | 'shadowtls'
  | 'tgweb';

export type SpecTone = 'default' | 'accent' | 'emerald' | 'amber' | 'muted';

export interface ProtocolSpecItem {
  label: string;
  value: string;
  mono?: boolean;
  tone?: SpecTone;
  title?: string;
}

export interface ProtocolFlag {
  label: string;
  tone?: 'default' | 'emerald' | 'amber' | 'accent';
  title?: string;
}

export interface ProtocolEndpointMeta {
  label: string;
  display: string;
  copyValue: string;
  secondary?: {
    label: string;
    value: string;
    copyValue?: string;
  };
}

export interface ProtocolVisualTheme {
  railClass: string;
  cardHoverClass: string;
  headerWashClass: string;
  iconBoxClass: string;
  accentTextClass: string;
}

export interface ProtocolCardMeta {
  protocol: Protocol;
  protocolLabel: string;
  coreLabel: string;
  icon: ProtocolIconKey;
  theme: ProtocolVisualTheme;
  subtitle: string;
  endpoint: ProtocolEndpointMeta;
  specs: ProtocolSpecItem[];
  flags: ProtocolFlag[];
  compactSummary: string;
  searchText: string;
}

const PROTOCOL_THEMES: Record<Protocol, ProtocolVisualTheme> = {
  VLESS: {
    railClass: 'bg-sky-400 shadow-[0_0_12px_rgba(56,189,248,0.55)]',
    cardHoverClass: 'hover:border-sky-500/45',
    headerWashClass: 'from-sky-500/10 via-transparent to-transparent',
    iconBoxClass: 'border-sky-500/30 bg-sky-500/12 text-sky-600 dark:text-sky-400',
    accentTextClass: 'text-sky-600 dark:text-sky-400',
  },
  VMESS: {
    railClass: 'bg-indigo-400 shadow-[0_0_12px_rgba(129,140,248,0.55)]',
    cardHoverClass: 'hover:border-indigo-500/45',
    headerWashClass: 'from-indigo-500/10 via-transparent to-transparent',
    iconBoxClass: 'border-indigo-500/30 bg-indigo-500/12 text-indigo-600 dark:text-indigo-400',
    accentTextClass: 'text-indigo-600 dark:text-indigo-400',
  },
  TROJAN: {
    railClass: 'bg-amber-400 shadow-[0_0_12px_rgba(251,191,36,0.55)]',
    cardHoverClass: 'hover:border-amber-500/45',
    headerWashClass: 'from-amber-500/10 via-transparent to-transparent',
    iconBoxClass: 'border-amber-500/30 bg-amber-500/12 text-amber-600 dark:text-amber-400',
    accentTextClass: 'text-amber-600 dark:text-amber-400',
  },
  SHADOWSOCKS: {
    railClass: 'bg-violet-400 shadow-[0_0_12px_rgba(167,139,250,0.55)]',
    cardHoverClass: 'hover:border-violet-500/45',
    headerWashClass: 'from-violet-500/10 via-transparent to-transparent',
    iconBoxClass: 'border-violet-500/30 bg-violet-500/12 text-violet-600 dark:text-violet-400',
    accentTextClass: 'text-violet-600 dark:text-violet-400',
  },
  HYSTERIA2: {
    railClass: 'bg-emerald-400 shadow-[0_0_12px_rgba(52,211,153,0.55)]',
    cardHoverClass: 'hover:border-emerald-500/45',
    headerWashClass: 'from-emerald-500/10 via-transparent to-transparent',
    iconBoxClass: 'border-emerald-500/30 bg-emerald-500/12 text-emerald-600 dark:text-emerald-400',
    accentTextClass: 'text-emerald-600 dark:text-emerald-400',
  },
  NAIVEPROXY: {
    railClass: 'bg-cyan-400 shadow-[0_0_12px_rgba(34,211,238,0.55)]',
    cardHoverClass: 'hover:border-cyan-500/45',
    headerWashClass: 'from-cyan-500/10 via-transparent to-transparent',
    iconBoxClass: 'border-cyan-500/30 bg-cyan-500/12 text-cyan-600 dark:text-cyan-400',
    accentTextClass: 'text-cyan-600 dark:text-cyan-400',
  },
  MIERU: {
    railClass: 'bg-rose-400 shadow-[0_0_12px_rgba(251,113,133,0.55)]',
    cardHoverClass: 'hover:border-rose-500/45',
    headerWashClass: 'from-rose-500/10 via-transparent to-transparent',
    iconBoxClass: 'border-rose-500/30 bg-rose-500/12 text-rose-600 dark:text-rose-400',
    accentTextClass: 'text-rose-600 dark:text-rose-400',
  },
  TUIC: {
    railClass: 'bg-teal-400 shadow-[0_0_12px_rgba(45,212,191,0.55)]',
    cardHoverClass: 'hover:border-teal-500/45',
    headerWashClass: 'from-teal-500/10 via-transparent to-transparent',
    iconBoxClass: 'border-teal-500/30 bg-teal-500/12 text-teal-600 dark:text-teal-400',
    accentTextClass: 'text-teal-600 dark:text-teal-400',
  },
  ANYTLS: {
    railClass: 'bg-fuchsia-400 shadow-[0_0_12px_rgba(232,121,249,0.55)]',
    cardHoverClass: 'hover:border-fuchsia-500/45',
    headerWashClass: 'from-fuchsia-500/10 via-transparent to-transparent',
    iconBoxClass: 'border-fuchsia-500/30 bg-fuchsia-500/12 text-fuchsia-600 dark:text-fuchsia-400',
    accentTextClass: 'text-fuchsia-600 dark:text-fuchsia-400',
  },
  SHADOWTLS: {
    railClass: 'bg-orange-400 shadow-[0_0_12px_rgba(251,146,60,0.55)]',
    cardHoverClass: 'hover:border-orange-500/45',
    headerWashClass: 'from-orange-500/10 via-transparent to-transparent',
    iconBoxClass: 'border-orange-500/30 bg-orange-500/12 text-orange-600 dark:text-orange-400',
    accentTextClass: 'text-orange-600 dark:text-orange-400',
  },
  TGWEB: {
    railClass: 'bg-blue-400 shadow-[0_0_12px_rgba(96,165,250,0.55)]',
    cardHoverClass: 'hover:border-blue-500/45',
    headerWashClass: 'from-blue-500/10 via-transparent to-transparent',
    iconBoxClass: 'border-blue-500/30 bg-blue-500/12 text-blue-600 dark:text-blue-400',
    accentTextClass: 'text-blue-600 dark:text-blue-400',
  },
};

function getCoreDisplayLabel(protocol: Protocol): string {
  const core = getCoreForProtocol(protocol);
  switch (core) {
    case 'xray':
      return 'Xray';
    case 'singbox':
      return 'Sing-box';
    case 'naive':
      return 'Caddy / Naive';
    case 'mieru':
      return 'Mieru (mita)';
    case 'telemt':
      return 'telemt';
  }
}

function asObj(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function asStr(value: unknown, fallback = ''): string {
  if (typeof value === 'string') return value.trim();
  if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  return fallback;
}

function asBool(value: unknown, fallback = false): boolean {
  if (typeof value === 'boolean') return value;
  if (typeof value === 'string') {
    const lower = value.trim().toLowerCase();
    if (lower === 'true' || lower === '1') return true;
    if (lower === 'false' || lower === '0') return false;
  }
  return fallback;
}

function asNum(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string' && value.trim()) {
    const parsed = Number(value.trim());
    if (Number.isFinite(parsed)) return parsed;
  }
  return undefined;
}

function asStrArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.map((v) => asStr(v)).filter(Boolean);
}

interface XrayStreamDetails {
  networkRaw: string;
  netUpper: string;
  security: string;
  sni: string;
  realityDest: string;
  xhttpMode: string;
  transportPath: string;
  fingerprint: string;
  alpn: string;
  hasMldsa65: boolean;
  hasEch: boolean;
  sockoptFlags: string[];
}

function extractXrayStreamDetails(
  stream: Record<string, unknown>,
  settings: Record<string, unknown>,
): XrayStreamDetails {
  const networkRaw = asStr(stream.network, 'tcp').toLowerCase() || 'tcp';
  const netUpper = networkRaw.toUpperCase();
  const security = asStr(stream.security, 'none').toUpperCase() || 'NONE';

  const realitySettings = asObj(stream.realitySettings);
  const tlsSettings = asObj(stream.tlsSettings);
  const xhttpSettings = asObj(stream.xhttpSettings ?? stream.splithttpSettings);
  const wsSettings = asObj(stream.wsSettings);
  const httpupgradeSettings = asObj(stream.httpupgradeSettings);
  const grpcSettings = asObj(stream.grpcSettings);
  const httpSettings = asObj(stream.httpSettings);
  const sockopt = asObj(stream.sockopt ?? settings.sockopt);

  const serverNames = asStrArray(realitySettings.serverNames);
  const sni =
    serverNames[0] ||
    asStr(tlsSettings.serverName) ||
    asStr(stream.sni) ||
    asStr(settings.sni);

  const realityDest =
    asStr(realitySettings.target) ||
    asStr(realitySettings.dest) ||
    asStr(stream.dest);

  const xhttpMode =
    networkRaw === 'xhttp' || networkRaw === 'splithttp' ? asStr(xhttpSettings.mode) : '';

  let transportPath = '';
  if (networkRaw === 'xhttp' || networkRaw === 'splithttp') {
    transportPath = asStr(xhttpSettings.path);
  } else if (networkRaw === 'ws') {
    transportPath = asStr(wsSettings.path);
  } else if (networkRaw === 'httpupgrade') {
    transportPath = asStr(httpupgradeSettings.path);
  } else if (networkRaw === 'grpc') {
    transportPath = asStr(grpcSettings.serviceName);
  } else if (networkRaw === 'h2' || networkRaw === 'http') {
    transportPath = asStr(httpSettings.path);
  }

  const fingerprint =
    asStr(realitySettings.fingerprint) ||
    asStr(tlsSettings.fingerprint) ||
    asStr(stream.fingerprint);

  const alpnList = asStrArray(tlsSettings.alpn ?? realitySettings.alpn ?? stream.alpn);
  const alpn = alpnList.length ? alpnList.join(', ') : asStr(tlsSettings.alpn ?? stream.alpn);

  const hasMldsa65 = Boolean(
    asStr(realitySettings.mldsa65Seed) ||
      asStr(stream.mldsa65Seed) ||
      asStr(realitySettings.mldsa65Verify) ||
      asStr(stream.mldsa65Verify),
  );

  const hasEch = Boolean(
    asStr(tlsSettings.echServerKeys) ||
      asStr(stream.echServerKeys) ||
      asStr(tlsSettings.echConfig) ||
      asStr(settings.echConfig) ||
      asStr(settings.echServerKeys),
  );

  const sockoptFlags: string[] = [];
  if (asBool(sockopt.tcpMptcp, false)) sockoptFlags.push('MPTCP');
  if (asBool(sockopt.tcpFastOpen, false)) sockoptFlags.push('TFO');

  return {
    networkRaw,
    netUpper,
    security,
    sni,
    realityDest,
    xhttpMode,
    transportPath,
    fingerprint,
    alpn,
    hasMldsa65,
    hasEch,
    sockoptFlags,
  };
}

function buildXrayFlags(
  inbound: Inbound,
  isRu: boolean,
  xray: XrayStreamDetails,
): ProtocolFlag[] {
  const flags: ProtocolFlag[] = [];
  if (!inbound.sniffing) {
    flags.push({ label: 'sniff:OFF', tone: 'amber' });
  }
  if (inbound.portShares && inbound.portShares.length > 0) {
    flags.push({
      label: isRu ? `Доп. порты +${inbound.portShares.length}` : `Shares +${inbound.portShares.length}`,
      tone: 'accent',
    });
  }
  if (xray.hasMldsa65) {
    flags.push({ label: 'ML-DSA-65', tone: 'emerald' });
  }
  if (xray.hasEch) {
    flags.push({ label: 'ECH', tone: 'emerald' });
  }
  for (const sockFlag of xray.sockoptFlags) {
    flags.push({ label: sockFlag, tone: 'default' });
  }
  if (inbound.routing?.blockTorrent) {
    flags.push({ label: 'No-Torrent', tone: 'amber' });
  }
  if (inbound.routing?.blockAds) {
    flags.push({ label: 'No-Ads', tone: 'default' });
  }
  return flags;
}

export function extractProtocolCardMeta(
  inbound: Inbound,
  isRu: boolean,
  fallbackHost?: string,
): ProtocolCardMeta {
  const protocol = inbound.protocol;
  const settings = asObj(inbound.settings);
  const stream = asObj(inbound.stream);
  const host = inbound.node?.host || fallbackHost || inbound.listen || '0.0.0.0';
  const defaultEndpointStr = `${host}:${inbound.port}`;
  const theme = PROTOCOL_THEMES[protocol] ?? PROTOCOL_THEMES.VLESS;
  const coreLabel = getCoreDisplayLabel(protocol);
  const protocolLabel = PROTOCOL_LABELS[protocol] ?? protocol;

  let icon: ProtocolIconKey = 'vless';
  let subtitle = coreLabel;
  let endpoint: ProtocolEndpointMeta = {
    label: isRu ? 'Адрес' : 'Endpoint',
    display: defaultEndpointStr,
    copyValue: defaultEndpointStr,
  };
  let specs: ProtocolSpecItem[] = [];
  let flags: ProtocolFlag[] = [];
  const searchExtra: string[] = [];

  if (protocol === 'VLESS') {
    const xray = extractXrayStreamDetails(stream, settings);
    const flow = asStr(settings.flow);
    const decryption = asStr(settings.decryption, 'none');
    const hasPqEnc = Boolean(decryption && decryption !== 'none');

    icon = xray.security === 'REALITY' ? 'reality' : 'vless';
    subtitle = `${xray.security !== 'NONE' ? xray.security : 'PLAIN'} · ${xray.netUpper}${xray.xhttpMode ? ` (${xray.xhttpMode})` : ''}`;

    const transportValue = [
      xray.netUpper,
      xray.xhttpMode,
      xray.transportPath,
    ]
      .filter(Boolean)
      .join(' · ');

    const fourthSpec: ProtocolSpecItem = hasPqEnc
      ? {
          label: isRu ? 'Шифр VLESS' : 'VLESS Enc',
          value: decryption.toLowerCase().includes('mlkem') ? 'ML-KEM-768 PQ' : 'X25519 / PQ',
          tone: 'accent',
          title: decryption,
        }
      : flow
        ? {
            label: isRu ? 'Поток (Flow)' : 'Flow',
            value: flow,
            mono: true,
            tone: 'accent',
          }
        : xray.security === 'REALITY'
          ? {
              label: isRu ? 'Цель Reality' : 'Reality Dest',
              value: xray.realityDest || '—',
              mono: true,
            }
          : {
              label: 'ALPN',
              value: xray.alpn || '—',
              mono: true,
            };

    specs = [
      {
        label: isRu ? 'Транспорт' : 'Transport',
        value: transportValue || 'TCP',
        mono: true,
        tone: 'accent',
      },
      {
        label: isRu ? 'Защита' : 'Security',
        value: xray.fingerprint ? `${xray.security} · ${xray.fingerprint}` : xray.security,
        mono: true,
        tone: xray.security === 'NONE' ? 'amber' : 'emerald',
      },
      {
        label: 'SNI',
        value: xray.sni || '—',
        mono: true,
        title: xray.realityDest ? `${xray.sni || '—'} → ${xray.realityDest}` : xray.sni || undefined,
      },
      fourthSpec,
    ];
    flags = buildXrayFlags(inbound, isRu, xray);
    searchExtra.push(xray.netUpper, xray.security, xray.sni, xray.realityDest, xray.transportPath, flow, decryption);
  } else if (protocol === 'VMESS') {
    const xray = extractXrayStreamDetails(stream, settings);
    const cipher = asStr(settings.security, 'auto') || 'auto';
    const alterId = asNum(settings.alterId) ?? 0;

    icon = 'vmess';
    subtitle = `${xray.security !== 'NONE' ? xray.security : 'PLAIN'} · ${xray.netUpper}`;

    specs = [
      {
        label: isRu ? 'Транспорт' : 'Transport',
        value: [xray.netUpper, xray.transportPath].filter(Boolean).join(' · ') || 'TCP',
        mono: true,
        tone: 'accent',
      },
      {
        label: isRu ? 'Шифр' : 'Cipher',
        value: cipher,
        mono: true,
      },
      {
        label: isRu ? 'Защита' : 'Security',
        value: xray.security,
        mono: true,
        tone: xray.security === 'NONE' ? 'amber' : 'emerald',
      },
      alterId > 0
        ? {
            label: 'AlterID',
            value: String(alterId),
            mono: true,
          }
        : {
            label: 'SNI',
            value: xray.sni || '—',
            mono: true,
          },
    ];
    flags = buildXrayFlags(inbound, isRu, xray);
    searchExtra.push(xray.netUpper, xray.security, xray.sni, xray.transportPath, cipher);
  } else if (protocol === 'TROJAN') {
    const xray = extractXrayStreamDetails(stream, settings);
    const fallbacksCount = Array.isArray(settings.fallbacks) ? settings.fallbacks.length : 0;
    const secLabel = xray.security !== 'NONE' ? xray.security : 'TLS';

    icon = 'trojan';
    subtitle = `${secLabel} · ${xray.netUpper}`;

    specs = [
      {
        label: isRu ? 'Транспорт' : 'Transport',
        value: [xray.netUpper, xray.transportPath].filter(Boolean).join(' · ') || 'TCP',
        mono: true,
        tone: 'accent',
      },
      {
        label: isRu ? 'Защита' : 'Security',
        value: xray.fingerprint ? `${secLabel} · ${xray.fingerprint}` : secLabel,
        mono: true,
        tone: 'emerald',
      },
      {
        label: 'SNI',
        value: xray.sni || '—',
        mono: true,
      },
      fallbacksCount > 0
        ? {
            label: isRu ? 'Фолбэки' : 'Fallbacks',
            value: `${fallbacksCount} ${isRu ? 'прав.' : 'rules'}`,
            mono: true,
            tone: 'accent',
          }
        : {
            label: 'ALPN',
            value: xray.alpn || '—',
            mono: true,
          },
    ];
    flags = buildXrayFlags(inbound, isRu, xray);
    searchExtra.push(xray.netUpper, secLabel, xray.sni, xray.transportPath, xray.alpn);
  } else if (protocol === 'SHADOWSOCKS') {
    const xray = extractXrayStreamDetails(stream, settings);
    const method = asStr(settings.method, '2022-blake3-aes-128-gcm');
    const isSs2022 = method.startsWith('2022-');
    const ssNet = asStr(settings.network, 'tcp,udp').toUpperCase().replace(/,/g, ' + ');
    const hasShadowTlsPlugin = Boolean(asStr(settings.shadowtlsPassword));

    icon = 'shadowsocks';
    subtitle = `${isSs2022 ? 'SS-2022' : 'AEAD'} · ${ssNet}`;

    specs = [
      {
        label: isRu ? 'Шифр' : 'Cipher',
        value: method,
        mono: true,
        tone: isSs2022 ? 'accent' : 'default',
      },
      {
        label: isRu ? 'Сеть' : 'Network',
        value: ssNet,
        mono: true,
      },
      {
        label: isRu ? 'Стандарт' : 'Edition',
        value: isSs2022 ? 'SS-2022 (BLAKE3)' : 'Legacy AEAD',
        tone: isSs2022 ? 'emerald' : 'muted',
      },
      hasShadowTlsPlugin || xray.networkRaw !== 'tcp'
        ? {
            label: isRu ? 'Обёртка' : 'Wrapper',
            value: hasShadowTlsPlugin ? 'ShadowTLS' : xray.netUpper,
            mono: true,
            tone: 'accent',
          }
        : {
            label: 'Listen',
            value: inbound.listen || '0.0.0.0',
            mono: true,
            tone: 'muted',
          },
    ];
    flags = buildXrayFlags(inbound, isRu, xray);
    searchExtra.push(method, ssNet, isSs2022 ? 'ss-2022' : 'aead');
  } else if (protocol === 'HYSTERIA2') {
    const obfs = asObj(settings.obfs);
    const obfsType = asStr(obfs.type, 'none').toLowerCase();
    const portRange = asStr(settings.portRange);
    const bw = asObj(settings.bandwidth);
    const up = asNum(settings.uploadMbps ?? settings.maxUploadSpeed ?? bw.up);
    const down = asNum(settings.downloadMbps ?? settings.maxDownloadSpeed ?? bw.down);
    const ignoreClientBw = asBool(settings.ignoreClientBandwidth, false);
    const bbrProfile = asStr(settings.bbrProfile);
    const hy2Sni = asStr(settings.sni);
    const allowInsecure = asBool(settings.allowInsecure, false);
    const hasMasquerade = Boolean(
      typeof settings.masquerade === 'object' ? settings.masquerade : asStr(settings.masquerade),
    );
    const hasEch = Boolean(asStr(settings.echConfig) || asStr(settings.echKeyPath));

    icon = 'hysteria2';
    subtitle = [
      'QUIC · UDP',
      portRange ? (isRu ? 'Hopping' : 'Port Hop') : '',
      obfsType !== 'none' ? obfsType : '',
    ]
      .filter(Boolean)
      .join(' · ');

    endpoint = {
      label: isRu ? 'Адрес UDP' : 'UDP Endpoint',
      display: defaultEndpointStr,
      copyValue: defaultEndpointStr,
      ...(portRange ? { secondary: { label: 'Hop', value: portRange, copyValue: portRange } } : {}),
    };

    const obfsDisplay =
      obfsType === 'salamander'
        ? 'Salamander'
        : obfsType === 'gecko'
          ? 'Gecko'
          : isRu
            ? 'Отключена'
            : 'Direct';
    const bwDisplay = ignoreClientBw
      ? isRu
        ? 'Серверный (Ignore)'
        : 'Ignore Client BW'
      : up !== undefined || down !== undefined
        ? `↑${up ?? '∞'} / ↓${down ?? '∞'} Mbps`
        : isRu
          ? 'Авто (BBR)'
          : 'Auto (BBR)';

    specs = [
      {
        label: isRu ? 'Обфускация' : 'Obfuscation',
        value: obfsDisplay,
        tone: obfsType !== 'none' ? 'emerald' : 'muted',
      },
      {
        label: isRu ? 'Прыжки портов' : 'Port Hopping',
        value: portRange || `${isRu ? 'Один' : 'Single'} :${inbound.port}`,
        mono: true,
        tone: portRange ? 'accent' : 'muted',
      },
      {
        label: isRu ? 'Канал' : 'Bandwidth',
        value: bwDisplay,
        mono: true,
      },
      {
        label: hy2Sni ? 'SNI' : 'BBR',
        value: hy2Sni || (bbrProfile ? bbrProfile : '—'),
        mono: true,
      },
    ];

    if (bbrProfile) flags.push({ label: `BBR: ${bbrProfile}`, tone: 'accent' });
    if (hasMasquerade) flags.push({ label: isRu ? 'Маскарад' : 'Masquerade', tone: 'emerald' });
    if (hasEch) flags.push({ label: 'ECH', tone: 'emerald' });
    if (allowInsecure) flags.push({ label: 'Allow Insecure', tone: 'amber' });
    if (inbound.routing?.blockTorrent) flags.push({ label: 'No-Torrent', tone: 'amber' });
    searchExtra.push('quic', 'udp', obfsType, portRange, hy2Sni, bbrProfile);
  } else if (protocol === 'TUIC') {
    const cc = asStr(settings.congestion_control ?? settings.congestionControl, 'bbr').toUpperCase();
    const udpRelay = asStr(settings.udpRelayMode ?? settings.udp_relay_mode, 'native').toUpperCase();
    const zeroRtt = asBool(
      settings.zero_rtt ?? settings.zeroRtt ?? settings.zero_rtt_handshake ?? settings.zeroRttHandshake,
      false,
    );
    const heartbeat = asStr(settings.heartbeat);
    const tuicSni = asStr(settings.sni);
    const allowInsecure = asBool(settings.allowInsecure, false);

    icon = 'tuic';
    subtitle = `QUIC · ${cc}${zeroRtt ? ' · 0-RTT' : ''}`;

    specs = [
      {
        label: isRu ? 'Транспорт' : 'Transport',
        value: 'QUIC · UDP',
        mono: true,
      },
      {
        label: isRu ? 'Алгоритм' : 'Congestion',
        value: cc,
        mono: true,
        tone: 'accent',
      },
      {
        label: 'UDP Relay',
        value: udpRelay,
        mono: true,
      },
      {
        label: 'SNI',
        value: tuicSni || '—',
        mono: true,
      },
    ];

    if (zeroRtt) flags.push({ label: '0-RTT', tone: 'emerald' });
    if (heartbeat) flags.push({ label: `HB ${heartbeat}`, tone: 'default' });
    if (allowInsecure) flags.push({ label: 'Allow Insecure', tone: 'amber' });
    if (inbound.routing?.blockTorrent) flags.push({ label: 'No-Torrent', tone: 'amber' });
    searchExtra.push('quic', cc, udpRelay, tuicSni, zeroRtt ? '0-rtt' : '');
  } else if (protocol === 'NAIVEPROXY') {
    const proto = asStr(settings.proto, 'https').toLowerCase() === 'quic' ? 'quic' : 'https';
    const domain = asStr(settings.domain) || asStr(settings.sni);
    const tlsMode = asStr(settings.tlsMode, 'custom').toLowerCase() === 'letsencrypt' ? 'letsencrypt' : 'custom';
    const fallbackRoot = asStr(settings.fallbackRoot, '/var/www/html');
    const probeResistance = asBool(settings.probeResistance, true);
    const hideIp = asBool(settings.hideIp, true);
    const hideVia = asBool(settings.hideVia, true);
    const warpUpstream = asStr(settings.warpUpstream);

    icon = 'naiveproxy';
    subtitle = `Caddy · ${proto === 'quic' ? 'QUIC (H3)' : 'HTTPS (H2)'}`;

    const siteEndpoint = `${domain || host}:${inbound.port}`;
    endpoint = {
      label: isRu ? 'Сайт Caddy' : 'Caddy Site',
      display: siteEndpoint,
      copyValue: siteEndpoint,
      ...(domain ? { secondary: { label: 'IP', value: defaultEndpointStr, copyValue: defaultEndpointStr } } : {}),
    };

    specs = [
      {
        label: isRu ? 'Режим' : 'Mode',
        value: proto === 'quic' ? 'QUIC · HTTP/3' : 'HTTPS · HTTP/2',
        mono: true,
        tone: 'accent',
      },
      {
        label: isRu ? 'Домен' : 'Domain',
        value: domain || '—',
        mono: true,
      },
      {
        label: 'TLS',
        value: tlsMode === 'letsencrypt' ? "Let's Encrypt" : isRu ? 'Свой серт.' : 'Custom Cert',
        tone: 'emerald',
      },
      {
        label: isRu ? 'Заглушка' : 'Decoy Site',
        value: fallbackRoot,
        mono: true,
        tone: 'muted',
      },
    ];

    if (probeResistance) flags.push({ label: 'Probe-Resist', tone: 'emerald' });
    if (hideIp) flags.push({ label: 'Hide IP', tone: 'default' });
    else if (hideVia) flags.push({ label: 'Hide Via', tone: 'default' });
    if (warpUpstream) flags.push({ label: 'WARP', tone: 'accent', title: warpUpstream });
    searchExtra.push(proto, domain, tlsMode, fallbackRoot, warpUpstream, 'caddy');
  } else if (protocol === 'MIERU') {
    const rawTransport = asStr(settings.transport ?? settings.protocol, 'tcp').toLowerCase();
    const transportLabel =
      rawTransport === 'both' || rawTransport === 'tcp,udp' ? 'TCP + UDP' : rawTransport.toUpperCase();
    const portRange = asStr(settings.portRange);
    const domain = asStr(settings.domain) || asStr(settings.sni);
    const logLevel = asStr(settings.loggingLevel, 'WARN').toUpperCase();
    const mtu = asNum(settings.mtu);

    icon = 'mieru';
    subtitle = `Mita · ${transportLabel}${portRange ? (isRu ? ' · Диапазон' : ' · Port Range') : ''}`;

    const targetHost = domain || host;
    const primaryTarget = portRange ? `${targetHost}:${portRange}` : `${targetHost}:${inbound.port}`;
    endpoint = {
      label: portRange ? (isRu ? 'Диапазон' : 'Port Range') : isRu ? 'Адрес' : 'Endpoint',
      display: primaryTarget,
      copyValue: primaryTarget,
      ...(portRange ? { secondary: { label: 'Base', value: `:${inbound.port}` } } : {}),
    };

    specs = [
      {
        label: isRu ? 'Транспорт' : 'Transport',
        value: transportLabel,
        mono: true,
        tone: 'accent',
      },
      {
        label: portRange ? (isRu ? 'Диапазон портов' : 'Port Range') : isRu ? 'Порт' : 'Port',
        value: portRange || String(inbound.port),
        mono: true,
        tone: portRange ? 'emerald' : 'default',
      },
      {
        label: isRu ? 'Хост / Домен' : 'Host / Domain',
        value: targetHost,
        mono: true,
      },
      mtu !== undefined
        ? {
            label: 'MTU',
            value: String(mtu),
            mono: true,
          }
        : {
            label: isRu ? 'Логи' : 'Log Level',
            value: logLevel,
            mono: true,
            tone: 'muted',
          },
    ];

    if (portRange) flags.push({ label: 'Multi-Port', tone: 'emerald' });
    if (transportLabel === 'TCP + UDP') flags.push({ label: 'TCP+UDP', tone: 'accent' });
    if (mtu !== undefined) flags.push({ label: `Log ${logLevel}`, tone: 'default' });
    searchExtra.push(transportLabel, portRange, domain, logLevel, 'mita');
  } else if (protocol === 'TGWEB') {
    const domain = asStr(settings.domain);
    const publicAddr = asStr(settings.publicAddr);
    const basePath = asStr(settings.basePath).replace(/^\/+|\/+$/g, '');
    const carrier = asStr(settings.carrier, 'https-lanes');
    const secretMode = asStr(settings.secretMode, 'dd').toLowerCase() === 'plain' ? 'plain' : 'dd';
    const fallbackRoot = asStr(settings.fallbackRoot, '/var/www/html');

    icon = 'tgweb';
    subtitle = `MTProxy Web · ${carrier}`;

    const publicDisplay = domain
      ? basePath
        ? `${domain}/${basePath}`
        : domain
      : publicAddr || defaultEndpointStr;

    endpoint = {
      label: isRu ? 'Публичный URL' : 'Public Endpoint',
      display: publicDisplay,
      copyValue: domain ? `https://${publicDisplay}` : publicDisplay,
      secondary: {
        label: 'loopback',
        value: `127.0.0.1:${inbound.port}`,
        copyValue: `127.0.0.1:${inbound.port}`,
      },
    };

    specs = [
      {
        label: isRu ? 'Транспорт' : 'Carrier',
        value: carrier,
        mono: true,
        tone: 'accent',
      },
      {
        label: isRu ? 'Публичный IP' : 'Public Addr',
        value: publicAddr || (domain ? `${domain}:443` : '—'),
        mono: true,
      },
      {
        label: isRu ? 'Секрет / Путь' : 'Secret / Path',
        value: `${secretMode === 'dd' ? 'dd (FakeTLS)' : 'plain'}${basePath ? ` · /${basePath}` : ''}`,
        mono: true,
        tone: secretMode === 'dd' ? 'emerald' : 'default',
      },
      {
        label: isRu ? 'Заглушка' : 'Decoy Site',
        value: fallbackRoot,
        mono: true,
        tone: 'muted',
      },
    ];

    flags.push({ label: `Loopback :${inbound.port}`, tone: 'default' });
    if (domain) flags.push({ label: 'Caddy Front', tone: 'accent' });
    if (secretMode === 'dd') flags.push({ label: 'FakeTLS (dd)', tone: 'emerald' });
    searchExtra.push(domain, publicAddr, basePath, carrier, secretMode, fallbackRoot, 'telemt', 'mtproxy');
  } else if (protocol === 'ANYTLS') {
    const anySni = asStr(settings.sni);
    const checkInt = asStr(settings.idleSessionCheckInterval, '30s');
    const idleTimeout = asStr(settings.idleSessionTimeout, '30s');
    const minIdle = asNum(settings.minIdleSession) ?? 5;
    const paddingCount = Array.isArray(settings.paddingScheme)
      ? settings.paddingScheme.length
      : asStr(settings.paddingScheme)
        ? asStr(settings.paddingScheme).split('\n').filter(Boolean).length
        : 0;
    const alpnList = asStrArray(settings.alpn);
    const alpn = alpnList.length ? alpnList.join(', ') : asStr(settings.alpn);
    const allowInsecure = asBool(settings.allowInsecure, false);

    icon = 'anytls';
    subtitle = `TCP · TLS · Idle Pool (${minIdle})`;

    specs = [
      {
        label: isRu ? 'Транспорт' : 'Transport',
        value: 'TCP · TLS',
        mono: true,
      },
      {
        label: 'SNI',
        value: anySni || '—',
        mono: true,
      },
      {
        label: isRu ? 'Пул сессий' : 'Idle Pool',
        value: `${checkInt} / ${idleTimeout} (min ${minIdle})`,
        mono: true,
        tone: 'accent',
      },
      {
        label: isRu ? 'Паддинг' : 'Padding',
        value:
          paddingCount > 0
            ? `${paddingCount} ${isRu ? 'прав.' : 'rules'}`
            : isRu
              ? 'По умолч.'
              : 'Default',
        tone: paddingCount > 0 ? 'emerald' : 'muted',
      },
    ];

    if (alpn) flags.push({ label: `ALPN ${alpn}`, tone: 'default' });
    if (allowInsecure) flags.push({ label: 'Allow Insecure', tone: 'amber' });
    searchExtra.push(anySni, alpn, 'anytls');
  } else if (protocol === 'SHADOWTLS') {
    const version = asStr(settings.version ?? settings.shadowtlsVersion, '3');
    const hsObj = asObj(settings.handshake);
    const hsServer = asStr(settings.handshakeServer ?? hsObj.server ?? settings.sni, 'www.microsoft.com');
    const hsPort = asStr(settings.handshakePort ?? hsObj.server_port ?? hsObj.port, '443');
    const strictMode = asBool(settings.strictMode ?? settings.strict_mode, true);
    const detour = asStr(settings.detour);

    icon = 'shadowtls';
    subtitle = `ShadowTLS v${version}${strictMode ? ' · Strict' : ''}`;

    specs = [
      {
        label: isRu ? 'Версия' : 'Version',
        value: `v${version}`,
        mono: true,
        tone: 'accent',
      },
      {
        label: isRu ? 'Рукопожатие' : 'Handshake',
        value: `${hsServer}:${hsPort}`,
        mono: true,
      },
      {
        label: isRu ? 'Переброс' : 'Detour',
        value: detour || '—',
        mono: true,
      },
      {
        label: 'Strict Mode',
        value: strictMode ? 'ON' : 'OFF',
        mono: true,
        tone: strictMode ? 'emerald' : 'amber',
      },
    ];

    if (strictMode) flags.push({ label: 'Strict', tone: 'emerald' });
    searchExtra.push(`v${version}`, hsServer, detour, 'shadowtls');
  }

  const compactSummary = specs
    .slice(0, 3)
    .filter((s) => s.value && s.value !== '—')
    .map((s) => `${s.label}: ${s.value}`)
    .join(' · ');

  const searchText = [
    inbound.tag,
    protocol,
    protocolLabel,
    coreLabel,
    String(inbound.port),
    inbound.listen,
    inbound.node?.name ?? '',
    inbound.node?.host ?? '',
    inbound.remark ?? '',
    subtitle,
    endpoint.display,
    ...searchExtra,
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase();

  return {
    protocol,
    protocolLabel,
    coreLabel,
    icon,
    theme,
    subtitle,
    endpoint,
    specs,
    flags,
    compactSummary,
    searchText,
  };
}
