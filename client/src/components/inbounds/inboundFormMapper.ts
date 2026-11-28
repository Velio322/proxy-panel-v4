import type { Inbound, Protocol } from '@/types';
import {
  fromBase64Url,
  generateRealityKeypair,
  generateShortId,
  toBase64Url,
  x25519PublicFromPrivate,
} from '@/lib/core-utils';
import type {
  Hy2BbrProfile,
  Hy2ObfsType,
  InboundFormState,
  NetworkType,
  PortShareFormState,
  SecurityType,
  VlessFlowType,
} from '@/components/inbounds/types';

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' && !Array.isArray(v) ? { ...(v as Record<string, unknown>) } : {};
}

function asStr(v: unknown, fallback = ''): string {
  if (typeof v === 'string') return v;
  if (typeof v === 'number' && Number.isFinite(v)) return String(v);
  return fallback;
}

function asBool(v: unknown, fallback = false): boolean {
  return typeof v === 'boolean' ? v : fallback;
}

function asNumStr(v: unknown): string {
  if (typeof v === 'number' && Number.isFinite(v)) return String(v);
  if (typeof v === 'string' && v.trim() !== '' && Number.isFinite(Number(v))) return v.trim();
  return '';
}

function parseJsonSafe(raw: string): Record<string, unknown> | undefined {
  const trimmed = raw.trim();
  if (!trimmed) return undefined;
  try {
    const parsed = JSON.parse(trimmed);
    return parsed && typeof parsed === 'object' ? (parsed as Record<string, unknown>) : undefined;
  } catch {
    return undefined;
  }
}

export function createDefaultInboundForm(nodeId = ''): InboundFormState {
  const keys = generateRealityKeypair();
  return {
    nodeId,
    protocol: 'VLESS',
    tag: 'vless-reality-443',
    port: '443',
    listen: '0.0.0.0',
    remark: '',
    enable: true,
    sniffing: true,

    network: 'tcp',
    tcpHeaderType: 'none',
    tcpHeaderHost: '',
    tcpHeaderPath: '/',
    tcpAcceptProxy: false,
    wsPath: '/',
    wsHost: '',
    wsHeadersJson: '',
    wsAcceptProxy: false,
    wsHeartbeatPeriod: '',
    grpcServiceName: '',
    grpcAuthority: '',
    grpcMultiMode: false,
    grpcIdleTimeout: '',
    grpcHealthCheckTimeout: '',
    grpcPermitWithoutStream: false,
    grpcInitialWindowsSize: '',
    httpupgradePath: '/',
    httpupgradeHost: '',
    httpupgradeAcceptProxy: false,
    xhttpPath: '/',
    xhttpHost: '',
    xhttpMode: 'packet-up',
    xhttpMaxConcurrency: '',
    xhttpMaxConnections: '',
    xhttpCMaxReuseTimes: '',
    xhttpHMaxRequestTimes: '',
    xhttpHMaxReusableSecs: '',
    xhttpHKeepAlivePeriod: '',
    xhttpScMaxEachPostBytes: '',
    xhttpScMinPostsIntervalMs: '',
    xhttpScMaxBufferedPosts: '',
    xhttpScStreamUpServerSecs: '',
    xhttpXPaddingBytes: '100-1000',
    xhttpNoGrpcHeader: false,
    xhttpNoSseHeader: false,
    xhttpHeadersJson: '',
    xhttpDownloadSettingsJson: '',
    xhttpExtraJson: '',
    h2Path: '/',
    h2Host: '',
    kcpHeaderType: 'none',
    kcpSeed: '',
    kcpMtu: '',
    kcpTti: '',
    kcpUplinkCapacity: '',
    kcpDownlinkCapacity: '',
    kcpCongestion: false,
    kcpReadBufferSize: '',
    kcpWriteBufferSize: '',

    sniffingDestOverride: ['http', 'tls', 'quic', 'fakedns'],
    sniffingRouteOnly: true,
    sniffingMetadataOnly: false,
    sniffingDomainsExcluded: '',
    sockoptEnabled: false,
    sockoptMark: '0',
    sockoptV6Only: false,
    sockoptTproxy: 'off',
    sockoptTcpFastOpen: false,
    sockoptTcpMptcp: false,
    sockoptTcpKeepAliveInterval: '',
    sockoptTcpKeepAliveIdle: '',
    sockoptTcpCongestion: '',
    sockoptInterface: '',
    sockoptAcceptProxyProtocol: false,
    sockoptDomainStrategy: '',

    security: 'reality',
    sni: 'www.google.com',
    alpn: 'h2,http/1.1',
    fingerprint: 'chrome',
    allowInsecure: false,
    certFile: '',
    keyFile: '',
    pinnedPeerCertSha256: '',
    verifyPeerCertByName: '',
    echEnabled: false,
    echConfig: '',
    echKeyPath: '',
    tlsCipherSuites: '',
    tlsCurvePreferences: '',
    tlsOcspStapling: '',
    tlsEchServerKeys: '',
    tlsRejectUnknownSni: true,

    realityDest: 'www.google.com:443',
    realityServerNames: 'www.google.com',
    realityPrivateKey: keys.privateKey,
    realityPublicKey: keys.publicKey,
    realityShortIds: generateShortId(8),
    realitySpiderX: '/',
    realityXver: '0',
    realityShow: false,
    realityMaxTimeDiff: '',
    realityMinClientVer: '',
    realityMaxClientVer: '',
    realityLimitFallbackUploadJson: '',
    realityLimitFallbackDownloadJson: '',
    mldsa65Seed: '',
    mldsa65Verify: '',

    finalMaskEnabled: false,
    finalMaskJson: '',

    vlessFlow: 'xtls-rprx-vision',
    vlessDecryption: 'none',
    vlessEncryption: '',
    vlessRoute: '',
    vlessFallbacksJson: '',
    trojanFallbacksJson: '',

    vmessCipher: 'auto',
    alterId: '0',

    ssMethod: '2022-blake3-aes-128-gcm',
    ssNetwork: 'tcp,udp',

    password: '',
    uuid: '',

    hy2ObfsType: 'none',
    hy2ObfsPassword: '',
    hy2GeckoMinPacketSize: '',
    hy2GeckoMaxPacketSize: '',
    hy2BandwidthUp: '',
    hy2BandwidthDown: '',
    hy2MaxClient: '',
    hy2MaxStream: '',
    hy2IgnoreClientBandwidth: false,
    hy2Masquerade: '',
    hy2PortRange: '',
    hy2BbrProfile: 'none',
    disableChromeParrot: false,

    tuicCongestionControl: 'bbr',
    tuicZeroRtt: false,
    tuicAuthTimeout: '',
    tuicHeartbeat: '',
    tuicUdpRelayMode: 'native',

    naiveProto: 'https',
    naiveDomain: '',
    naiveEmail: '',
    naiveTlsMode: 'custom',
    naiveHideIp: true,
    naiveHideVia: true,
    naiveProbeResistance: true,
    naiveFallbackRoot: '/var/www/html',
    naiveWarpUpstream: '',

    mieruTransport: 'tcp',
    mieruPortRange: '',
    mieruLoggingLevel: 'WARN',
    mieruDomain: '',
    mieruMtu: '',

    tgwebDomain: '',
    tgwebPublicAddr: '',
    tgwebBasePath: '',
    tgwebCarrier: 'https-lanes',
    tgwebSecretMode: 'dd',
    tgwebFallbackRoot: '/var/www/html',

    anytlsPaddingScheme: '',
    anytlsIdleCheckInterval: '30s',
    anytlsIdleTimeout: '30s',
    anytlsMinIdleSession: '5',

    shadowtlsVersion: '3',
    shadowtlsHandshakeServer: 'www.microsoft.com',
    shadowtlsHandshakePort: '443',
    shadowtlsStrictMode: true,
    shadowtlsDetour: '',
    shadowtlsSni: '',

    routingBlockTorrent: false,
    routingBlockAds: false,

    portShares: [],

    rawSettings: {},
    rawStream: {},
    rawRouting: {},
  };
}

export function inboundToForm(inbound: Inbound): InboundFormState {
  const base = createDefaultInboundForm(inbound.nodeId);
  const settings = asObj(inbound.settings);
  const stream = asObj(inbound.stream);
  const routing = asObj(inbound.routing);

  const tlsSettings = asObj(stream.tlsSettings);
  const realitySettings = asObj(stream.realitySettings);
  const tcpSettings = asObj(stream.tcpSettings ?? stream.rawSettings);
  const tcpHeader = asObj(tcpSettings.header);
  const tcpReq = asObj(tcpHeader.request);
  const tcpReqHeaders = asObj(tcpReq.headers);
  const wsSettings = asObj(stream.wsSettings);
  const grpcSettings = asObj(stream.grpcSettings);
  const httpupgradeSettings = asObj(stream.httpupgradeSettings);
  const xhttpSettings = asObj(stream.xhttpSettings);
  const httpSettings = asObj(stream.httpSettings);
  const kcpSettings = asObj(stream.kcpSettings);
  const sockoptObj = asObj(stream.sockopt ?? settings.sockopt);
  const sniffingObj = asObj(settings.sniffing ?? stream.sniffing);
  const xhttpExtra = asObj(xhttpSettings.extra);
  const xmux = asObj(xhttpSettings.xmux ?? xhttpExtra.xmux);
  const obfs = asObj(settings.obfs);
  const bw = asObj(settings.bandwidth);
  const firstCert = Array.isArray(tlsSettings.certificates) && tlsSettings.certificates.length
    ? asObj(tlsSettings.certificates[0])
    : {};

  const rawNetwork = asStr(stream.network, 'tcp');
  const validNetworks: NetworkType[] = ['tcp', 'raw', 'ws', 'grpc', 'httpupgrade', 'xhttp', 'h2', 'kcp'];
  const network: NetworkType = (validNetworks.includes(rawNetwork as NetworkType) ? rawNetwork : 'tcp') as NetworkType;

  const rawSecurity = asStr(stream.security, 'none');
  const security: SecurityType =
    rawSecurity === 'tls' || rawSecurity === 'reality' ? rawSecurity : 'none';

  const rawFlow = asStr(settings.flow, 'none');
  const vlessFlow: VlessFlowType =
    rawFlow === 'xtls-rprx-vision' || rawFlow === 'xtls-rprx-vision-udp443' ? rawFlow : 'none';

  const rawObfsType = asStr(obfs.type, 'none');
  const hy2ObfsType: Hy2ObfsType =
    rawObfsType === 'salamander' || rawObfsType === 'gecko' ? rawObfsType : 'none';

  const serverNamesList = Array.isArray(realitySettings.serverNames)
    ? realitySettings.serverNames.map((v) => String(v).trim()).filter(Boolean)
    : Array.isArray(stream.serverNames)
      ? (stream.serverNames as unknown[]).map((v) => String(v).trim()).filter(Boolean)
      : [];

  const shortIdsList = Array.isArray(realitySettings.shortIds)
    ? realitySettings.shortIds.map((v) => String(v).trim()).filter(Boolean)
    : asStr(stream.shortId)
      ? [asStr(stream.shortId)]
      : [];

  const alpnRaw = tlsSettings.alpn ?? stream.alpn ?? settings.alpn;
  const alpn = Array.isArray(alpnRaw) ? alpnRaw.join(',') : asStr(alpnRaw, 'h2,http/1.1');

  const finalMaskObj = stream.finalMask ?? stream.finalmask ?? settings.finalMask;
  const wsHeaders = asObj(wsSettings.headers);
  const wsHost = asStr(wsSettings.host) || asStr(wsHeaders.Host);
  const restWsHeaders = { ...wsHeaders };
  delete restWsHeaders.Host;

  const portShares: PortShareFormState[] = Array.isArray(inbound.portShares)
    ? inbound.portShares.map((ps) => ({
        id: ps.id,
        protocol: ps.protocol,
        tag: ps.tag,
        host: ps.host ?? '',
        path: ps.path ?? '',
        enable: ps.enable,
        settings: asObj(ps.settings),
        stream: asObj(ps.stream),
      }))
    : [];

  const rawBbr = asStr(settings.bbrProfile, 'none');
  const hy2BbrProfile: Hy2BbrProfile =
    rawBbr === 'conservative' || rawBbr === 'standard' || rawBbr === 'aggressive' ? rawBbr : 'none';

  const pinnedRaw = tlsSettings.pinnedPeerCertSha256 ?? stream.pinnedPeerCertSha256;
  const pinnedStr = Array.isArray(pinnedRaw) ? pinnedRaw.join(',') : asStr(pinnedRaw);

  const verifyNameRaw = tlsSettings.verifyPeerCertByName ?? stream.verifyPeerCertByName;
  const verifyNameStr = Array.isArray(verifyNameRaw) ? verifyNameRaw.join(',') : asStr(verifyNameRaw);

  const tcpReqHostRaw = tcpReqHeaders.Host;
  const tcpHeaderHost = Array.isArray(tcpReqHostRaw) ? asStr(tcpReqHostRaw[0]) : asStr(tcpReqHostRaw);
  const tcpReqPathRaw = tcpReq.path;
  const tcpHeaderPath = Array.isArray(tcpReqPathRaw) ? asStr(tcpReqPathRaw[0], '/') : asStr(tcpReqPathRaw, '/');
  const xhttpHeadersObj = xhttpSettings.headers ?? xhttpExtra.headers;
  const xhttpDlObj = xhttpSettings.downloadSettings ?? xhttpExtra.downloadSettings;
  const sockoptHasKeys = Object.keys(sockoptObj).length > 0;
  const sniffingDestOverride = Array.isArray(sniffingObj.destOverride)
    ? sniffingObj.destOverride.map(String).filter(Boolean)
    : ['http', 'tls', 'quic'];
  const curvePrefRaw = tlsSettings.curvePreferences ?? stream.curvePreferences;
  const kcpHeaderObj = asObj(kcpSettings.header);

  return {
    ...base,
    id: inbound.id,
    nodeId: inbound.nodeId,
    protocol: inbound.protocol,
    tag: inbound.tag,
    port: String(inbound.port),
    listen: inbound.listen || '0.0.0.0',
    remark: inbound.remark ?? '',
    enable: inbound.enable,
    sniffing: inbound.sniffing ?? true,

    network,
    tcpHeaderType: asStr(tcpHeader.type, 'none') === 'http' ? 'http' : 'none',
    tcpHeaderHost,
    tcpHeaderPath,
    tcpAcceptProxy: asBool(tcpSettings.acceptProxyProtocol, false),
    wsPath: asStr(wsSettings.path, '/'),
    wsHost,
    wsHeadersJson: Object.keys(restWsHeaders).length ? JSON.stringify(restWsHeaders, null, 2) : '',
    wsAcceptProxy: asBool(wsSettings.acceptProxyProtocol, false),
    wsHeartbeatPeriod: asNumStr(wsSettings.heartbeatPeriod),
    grpcServiceName: asStr(grpcSettings.serviceName),
    grpcAuthority: asStr(grpcSettings.authority),
    grpcMultiMode: asBool(grpcSettings.multiMode, false),
    grpcIdleTimeout: asNumStr(grpcSettings.idle_timeout),
    grpcHealthCheckTimeout: asNumStr(grpcSettings.health_check_timeout),
    grpcPermitWithoutStream: asBool(grpcSettings.permit_without_stream, false),
    grpcInitialWindowsSize: asNumStr(grpcSettings.initial_windows_size),
    httpupgradePath: asStr(httpupgradeSettings.path, '/'),
    httpupgradeHost: asStr(httpupgradeSettings.host),
    httpupgradeAcceptProxy: asBool(httpupgradeSettings.acceptProxyProtocol, false),
    xhttpPath: asStr(xhttpSettings.path, '/'),
    xhttpHost: asStr(xhttpSettings.host),
    xhttpMode: (asStr(xhttpSettings.mode, 'packet-up') as InboundFormState['xhttpMode']) || 'packet-up',
    xhttpMaxConcurrency: asNumStr(xmux.maxConcurrency),
    xhttpMaxConnections: asNumStr(xmux.maxConnections),
    xhttpCMaxReuseTimes: asNumStr(xmux.cMaxReuseTimes),
    xhttpHMaxRequestTimes: asNumStr(xmux.hMaxRequestTimes),
    xhttpHMaxReusableSecs: asNumStr(xmux.hMaxReusableSecs),
    xhttpHKeepAlivePeriod: asNumStr(xmux.hKeepAlivePeriod),
    xhttpScMaxEachPostBytes: asNumStr(xhttpSettings.scMaxEachPostBytes ?? xhttpExtra.scMaxEachPostBytes),
    xhttpScMinPostsIntervalMs: asNumStr(xhttpSettings.scMinPostsIntervalMs ?? xhttpExtra.scMinPostsIntervalMs),
    xhttpScMaxBufferedPosts: asNumStr(xhttpSettings.scMaxBufferedPosts ?? xhttpExtra.scMaxBufferedPosts),
    xhttpScStreamUpServerSecs: asStr(xhttpSettings.scStreamUpServerSecs ?? xhttpExtra.scStreamUpServerSecs),
    xhttpXPaddingBytes: asStr(xhttpSettings.xPaddingBytes ?? xhttpExtra.xPaddingBytes),
    xhttpNoGrpcHeader: asBool(xhttpSettings.noGRPCHeader ?? xhttpExtra.noGRPCHeader, false),
    xhttpNoSseHeader: asBool(xhttpSettings.noSSEHeader ?? xhttpExtra.noSSEHeader, false),
    xhttpHeadersJson: xhttpHeadersObj && typeof xhttpHeadersObj === 'object'
      ? JSON.stringify(xhttpHeadersObj, null, 2)
      : '',
    xhttpDownloadSettingsJson: xhttpDlObj ? JSON.stringify(xhttpDlObj, null, 2) : '',
    xhttpExtraJson: Object.keys(xhttpExtra).length ? JSON.stringify(xhttpExtra, null, 2) : '',
    h2Path: asStr(httpSettings.path, '/'),
    h2Host: Array.isArray(httpSettings.host)
      ? httpSettings.host.join(',')
      : asStr(httpSettings.host),
    kcpHeaderType: (asStr(kcpSettings.headerType ?? kcpHeaderObj.type, 'none') as InboundFormState['kcpHeaderType']) || 'none',
    kcpSeed: asStr(kcpSettings.seed),
    kcpMtu: asNumStr(kcpSettings.mtu),
    kcpTti: asNumStr(kcpSettings.tti),
    kcpUplinkCapacity: asNumStr(kcpSettings.uplinkCapacity),
    kcpDownlinkCapacity: asNumStr(kcpSettings.downlinkCapacity),
    kcpCongestion: asBool(kcpSettings.congestion, false),
    kcpReadBufferSize: asNumStr(kcpSettings.readBufferSize),
    kcpWriteBufferSize: asNumStr(kcpSettings.writeBufferSize),

    sniffingDestOverride: sniffingDestOverride.length
      ? sniffingDestOverride
      : ['http', 'tls', 'quic', 'fakedns'],
    sniffingRouteOnly: asBool(sniffingObj.routeOnly, true),
    sniffingMetadataOnly: asBool(sniffingObj.metadataOnly, false),
    sniffingDomainsExcluded: Array.isArray(sniffingObj.domainsExcluded)
      ? sniffingObj.domainsExcluded.map(String).join(', ')
      : '',
    sockoptEnabled: sockoptHasKeys,
    sockoptMark: asNumStr(sockoptObj.mark) || '0',
    sockoptV6Only: asBool(sockoptObj.V6Only, false),
    sockoptTproxy: (asStr(sockoptObj.tproxy, 'off') as InboundFormState['sockoptTproxy']) || 'off',
    sockoptTcpFastOpen: asBool(sockoptObj.tcpFastOpen, false),
    sockoptTcpMptcp: asBool(sockoptObj.tcpMptcp, false),
    sockoptTcpKeepAliveInterval: asNumStr(sockoptObj.tcpKeepAliveInterval),
    sockoptTcpKeepAliveIdle: asNumStr(sockoptObj.tcpKeepAliveIdle),
    sockoptTcpCongestion: asStr(sockoptObj.tcpcongestion ?? sockoptObj.tcpCongestion),
    sockoptInterface: asStr(sockoptObj.interface),
    sockoptAcceptProxyProtocol: asBool(sockoptObj.acceptProxyProtocol, false),
    sockoptDomainStrategy: asStr(sockoptObj.domainStrategy),

    security,
    sni:
      asStr(settings.sni) ||
      asStr(tlsSettings.serverName) ||
      asStr(stream.sni) ||
      serverNamesList[0] ||
      '',
    alpn,
    fingerprint: asStr(tlsSettings.fingerprint) || asStr(stream.fingerprint, 'chrome'),
    allowInsecure: asBool(
      settings.allowInsecure ?? settings.allow_insecure ?? tlsSettings.allowInsecure ?? stream.allowInsecure,
      false,
    ),
    certFile:
      asStr(settings.certFile) ||
      asStr(settings.certificate_path) ||
      asStr(firstCert.certificateFile),
    keyFile:
      asStr(settings.keyFile) ||
      asStr(settings.key_path) ||
      asStr(firstCert.keyFile),
    pinnedPeerCertSha256: pinnedStr,
    verifyPeerCertByName: verifyNameStr,
    echEnabled: Boolean(settings.echConfig || tlsSettings.echConfig || settings.echKeyPath || tlsSettings.echKeyPath || tlsSettings.echServerKeys || stream.echServerKeys),
    echConfig: asStr(settings.echConfig) || asStr(tlsSettings.echConfig),
    echKeyPath: asStr(settings.echKeyPath) || asStr(tlsSettings.echKeyPath),
    tlsCipherSuites: asStr(tlsSettings.cipherSuites),
    tlsCurvePreferences: Array.isArray(curvePrefRaw) ? curvePrefRaw.join(',') : asStr(curvePrefRaw),
    tlsOcspStapling: asNumStr(tlsSettings.ocspStapling),
    tlsEchServerKeys: asStr(tlsSettings.echServerKeys ?? stream.echServerKeys ?? settings.echServerKeys),
    tlsRejectUnknownSni: asBool(tlsSettings.rejectUnknownSni, true),

    realityDest: asStr(realitySettings.target) || asStr(realitySettings.dest) || asStr(stream.dest, 'www.google.com:443'),
    realityServerNames: serverNamesList.join(', '),
    realityPrivateKey: asStr(realitySettings.privateKey) || asStr(stream.privateKey),
    realityPublicKey: asStr(realitySettings.publicKey) || asStr(stream.publicKey),
    realityShortIds: shortIdsList.join(', '),
    realitySpiderX: asStr(realitySettings.spiderX) || asStr(stream.spiderX, '/'),
    realityXver: asNumStr(realitySettings.xver) || '0',
    realityShow: asBool(realitySettings.show, false),
    realityMaxTimeDiff: asNumStr(realitySettings.maxTimeDiff),
    realityMinClientVer: asStr(realitySettings.minClientVer),
    realityMaxClientVer: asStr(realitySettings.maxClientVer),
    realityLimitFallbackUploadJson: realitySettings.limitFallbackUpload
      ? JSON.stringify(realitySettings.limitFallbackUpload, null, 2)
      : '',
    realityLimitFallbackDownloadJson: realitySettings.limitFallbackDownload
      ? JSON.stringify(realitySettings.limitFallbackDownload, null, 2)
      : '',
    mldsa65Seed: asStr(realitySettings.mldsa65Seed) || asStr(stream.mldsa65Seed),
    mldsa65Verify: asStr(realitySettings.mldsa65Verify) || asStr(stream.mldsa65Verify),

    finalMaskEnabled: Boolean(finalMaskObj),
    finalMaskJson: finalMaskObj ? JSON.stringify(finalMaskObj, null, 2) : '',

    vlessFlow,
    vlessDecryption: asStr(settings.decryption, 'none') || 'none',
    vlessEncryption: asStr(settings.encryption),
    vlessRoute: asStr(settings.vlessRoute),
    vlessFallbacksJson:
      inbound.protocol === 'VLESS' && Array.isArray(settings.fallbacks)
        ? JSON.stringify(settings.fallbacks, null, 2)
        : '',
    trojanFallbacksJson:
      inbound.protocol === 'TROJAN' && Array.isArray(settings.fallbacks)
        ? JSON.stringify(settings.fallbacks, null, 2)
        : '',

    vmessCipher: (asStr(settings.security, 'auto') as InboundFormState['vmessCipher']) || 'auto',
    alterId: asNumStr(settings.alterId) || '0',

    ssMethod: (asStr(settings.method, '2022-blake3-aes-128-gcm') as InboundFormState['ssMethod']),
    ssNetwork: (asStr(settings.network, 'tcp,udp') as InboundFormState['ssNetwork']),

    password: asStr(settings.password),
    uuid: asStr(settings.uuid),

    hy2ObfsType,
    hy2ObfsPassword: asStr(obfs.password),
    hy2GeckoMinPacketSize: asNumStr(obfs.minPacketSize),
    hy2GeckoMaxPacketSize: asNumStr(obfs.maxPacketSize),
    hy2BandwidthUp: asNumStr(settings.uploadMbps ?? settings.maxUploadSpeed ?? bw.up),
    hy2BandwidthDown: asNumStr(settings.downloadMbps ?? settings.maxDownloadSpeed ?? bw.down),
    hy2MaxClient: asNumStr(settings.maxClient),
    hy2MaxStream: asNumStr(settings.maxStream),
    hy2IgnoreClientBandwidth: asBool(settings.ignoreClientBandwidth, false),
    hy2Masquerade:
      typeof settings.masquerade === 'object' && settings.masquerade
        ? JSON.stringify(settings.masquerade)
        : asStr(settings.masquerade),
    hy2PortRange: asStr(settings.portRange),
    hy2BbrProfile,
    disableChromeParrot: asBool(settings.disableChromeParrot ?? settings.disable_chrome_parrot, false),

    tuicCongestionControl: (asStr(settings.congestion_control ?? settings.congestionControl, 'bbr') as InboundFormState['tuicCongestionControl']),
    tuicZeroRtt: asBool(settings.zero_rtt ?? settings.zeroRtt ?? settings.zero_rtt_handshake ?? settings.zeroRttHandshake, false),
    tuicAuthTimeout: asStr(settings.authTimeout ?? settings.auth_timeout),
    tuicHeartbeat: asStr(settings.heartbeat),
    tuicUdpRelayMode: asStr(settings.udpRelayMode ?? settings.udp_relay_mode, 'native') === 'quic' ? 'quic' : 'native',

    naiveProto: asStr(settings.proto, 'https') === 'quic' ? 'quic' : 'https',
    naiveDomain: asStr(settings.domain),
    naiveEmail: asStr(settings.email),
    naiveTlsMode: asStr(settings.tlsMode, 'custom') === 'letsencrypt' ? 'letsencrypt' : 'custom',
    naiveHideIp: asBool(settings.hideIp, true),
    naiveHideVia: asBool(settings.hideVia, true),
    naiveProbeResistance: asBool(settings.probeResistance, true),
    naiveFallbackRoot: asStr(settings.fallbackRoot, '/var/www/html'),
    naiveWarpUpstream: asStr(settings.warpUpstream),

    mieruTransport: (asStr(settings.transport ?? settings.protocol, 'tcp') as InboundFormState['mieruTransport']),
    mieruPortRange: asStr(settings.portRange),
    mieruLoggingLevel: (asStr(settings.loggingLevel, 'WARN') as InboundFormState['mieruLoggingLevel']),
    mieruDomain: asStr(settings.domain),
    mieruMtu: asNumStr(settings.mtu),

    tgwebDomain: asStr(settings.domain),
    tgwebPublicAddr: asStr(settings.publicAddr),
    tgwebBasePath: asStr(settings.basePath),
    tgwebCarrier: (asStr(settings.carrier, 'https-lanes') as InboundFormState['tgwebCarrier']),
    tgwebSecretMode: asStr(settings.secretMode, 'dd') === 'plain' ? 'plain' : 'dd',
    tgwebFallbackRoot: asStr(settings.fallbackRoot, '/var/www/html'),

    anytlsPaddingScheme: Array.isArray(settings.paddingScheme)
      ? settings.paddingScheme.map(String).join('\n')
      : asStr(settings.paddingScheme),
    anytlsIdleCheckInterval: asStr(settings.idleSessionCheckInterval, '30s'),
    anytlsIdleTimeout: asStr(settings.idleSessionTimeout, '30s'),
    anytlsMinIdleSession: asNumStr(settings.minIdleSession) || '5',

    shadowtlsVersion: (asStr(settings.version ?? settings.shadowtlsVersion, '3') as InboundFormState['shadowtlsVersion']) || '3',
    shadowtlsHandshakeServer: asStr(settings.handshakeServer ?? settings.sni, 'www.microsoft.com'),
    shadowtlsHandshakePort: asNumStr(settings.handshakePort) || '443',
    shadowtlsStrictMode: asBool(settings.strictMode ?? settings.strict_mode, true),
    shadowtlsDetour: asStr(settings.detour),
    shadowtlsSni: asStr(settings.sni),

    routingBlockTorrent: asBool(routing.blockTorrent, false),
    routingBlockAds: asBool(routing.blockAds, false),

    portShares,
    rawSettings: settings,
    rawStream: stream,
    rawRouting: routing,
  };
}

function parsePositiveNum(val: string): number | undefined {
  const trimmed = val.trim();
  if (!trimmed) return undefined;
  const n = Number(trimmed);
  return Number.isFinite(n) && n >= 0 ? n : undefined;
}

function isXrayProtocol(protocol: Protocol): boolean {
  return protocol === 'VLESS' || protocol === 'VMESS' || protocol === 'TROJAN' || protocol === 'SHADOWSOCKS';
}

export function formToPayload(form: InboundFormState): Record<string, unknown> {
  const settings: Record<string, unknown> = { ...form.rawSettings };
  delete settings.clients;
  delete settings.users;

  const stream: Record<string, unknown> = { ...form.rawStream };
  const routing: Record<string, unknown> = {
    ...form.rawRouting,
    blockTorrent: form.routingBlockTorrent,
    blockAds: form.routingBlockAds,
  };

  const alpnList = form.alpn
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);

  if (form.protocol === 'VLESS') {
    settings.decryption = form.vlessDecryption.trim() || 'none';
    if (form.vlessEncryption.trim()) settings.encryption = form.vlessEncryption.trim();
    else delete settings.encryption;

    if (form.vlessFlow !== 'none') settings.flow = form.vlessFlow;
    else delete settings.flow;

    if (form.vlessRoute.trim()) settings.vlessRoute = form.vlessRoute.trim();
    else delete settings.vlessRoute;

    if (form.vlessFallbacksJson.trim()) {
      try {
        const fb = JSON.parse(form.vlessFallbacksJson.trim());
        if (Array.isArray(fb)) settings.fallbacks = fb;
      } catch {
        // leave existing fallbacks untouched on invalid JSON
      }
    } else {
      delete settings.fallbacks;
    }
  } else if (form.protocol === 'VMESS') {
    settings.security = form.vmessCipher;
    settings.alterId = Math.max(0, Math.floor(Number(form.alterId) || 0));
  } else if (form.protocol === 'TROJAN') {
    if (form.password.trim()) settings.password = form.password.trim();
    if (form.certFile.trim()) settings.certFile = form.certFile.trim();
    if (form.keyFile.trim()) settings.keyFile = form.keyFile.trim();
    if (form.trojanFallbacksJson.trim()) {
      try {
        const fb = JSON.parse(form.trojanFallbacksJson.trim());
        if (Array.isArray(fb)) settings.fallbacks = fb;
      } catch {
        // ignore invalid json
      }
    } else {
      delete settings.fallbacks;
    }
  } else if (form.protocol === 'SHADOWSOCKS') {
    settings.method = form.ssMethod;
    settings.network = form.ssNetwork;
    if (form.password.trim()) settings.password = form.password.trim();
  } else if (form.protocol === 'HYSTERIA2') {
    if (form.password.trim()) settings.password = form.password.trim();
    if (form.sni.trim()) settings.sni = form.sni.trim();
    else delete settings.sni;
    settings.allowInsecure = form.allowInsecure;
    if (form.certFile.trim()) settings.certFile = form.certFile.trim();
    if (form.keyFile.trim()) settings.keyFile = form.keyFile.trim();

    if (form.hy2ObfsType === 'none') {
      delete settings.obfs;
    } else if (form.hy2ObfsType === 'salamander') {
      settings.obfs = {
        type: 'salamander',
        password: form.hy2ObfsPassword.trim(),
      };
    } else {
      const minP = parsePositiveNum(form.hy2GeckoMinPacketSize);
      const maxP = parsePositiveNum(form.hy2GeckoMaxPacketSize);
      settings.obfs = {
        type: 'gecko',
        ...(form.hy2ObfsPassword.trim() ? { password: form.hy2ObfsPassword.trim() } : {}),
        ...(minP !== undefined ? { minPacketSize: minP } : {}),
        ...(maxP !== undefined ? { maxPacketSize: maxP } : {}),
      };
    }

    delete settings.bandwidth;
    const up = parsePositiveNum(form.hy2BandwidthUp);
    const down = parsePositiveNum(form.hy2BandwidthDown);
    if (up !== undefined) settings.uploadMbps = up;
    else delete settings.uploadMbps;
    if (down !== undefined) settings.downloadMbps = down;
    else delete settings.downloadMbps;

    const maxClient = parsePositiveNum(form.hy2MaxClient);
    const maxStream = parsePositiveNum(form.hy2MaxStream);
    if (maxClient !== undefined) settings.maxClient = maxClient;
    else delete settings.maxClient;
    if (maxStream !== undefined) settings.maxStream = maxStream;
    else delete settings.maxStream;

    settings.ignoreClientBandwidth = form.hy2IgnoreClientBandwidth;
    if (form.hy2Masquerade.trim()) {
      const masqObj = parseJsonSafe(form.hy2Masquerade);
      settings.masquerade = masqObj ?? form.hy2Masquerade.trim();
    } else {
      delete settings.masquerade;
    }

    if (form.hy2PortRange.trim()) settings.portRange = form.hy2PortRange.trim();
    else delete settings.portRange;

    if (form.hy2BbrProfile !== 'none') settings.bbrProfile = form.hy2BbrProfile;
    else delete settings.bbrProfile;

    settings.disableChromeParrot = form.disableChromeParrot;
    if (form.echEnabled && form.echConfig.trim()) settings.echConfig = form.echConfig.trim();
    else delete settings.echConfig;
    if (form.echEnabled && form.echKeyPath.trim()) settings.echKeyPath = form.echKeyPath.trim();
    else delete settings.echKeyPath;
  } else if (form.protocol === 'TUIC') {
    if (form.uuid.trim()) settings.uuid = form.uuid.trim();
    if (form.password.trim()) settings.password = form.password.trim();
    if (form.sni.trim()) settings.sni = form.sni.trim();
    settings.allowInsecure = form.allowInsecure;
    settings.congestion_control = form.tuicCongestionControl;
    settings.zero_rtt = form.tuicZeroRtt;
    settings.zero_rtt_handshake = form.tuicZeroRtt;
    if (form.tuicAuthTimeout.trim()) settings.authTimeout = form.tuicAuthTimeout.trim();
    else delete settings.authTimeout;
    if (form.tuicHeartbeat.trim()) settings.heartbeat = form.tuicHeartbeat.trim();
    else delete settings.heartbeat;
    settings.udpRelayMode = form.tuicUdpRelayMode;
    if (form.certFile.trim()) settings.certFile = form.certFile.trim();
    if (form.keyFile.trim()) settings.keyFile = form.keyFile.trim();
  } else if (form.protocol === 'NAIVEPROXY') {
    settings.proto = form.naiveProto;
    if (form.naiveDomain.trim()) settings.domain = form.naiveDomain.trim();
    const naiveSni = form.naiveDomain.trim() || form.sni.trim();
    if (naiveSni) settings.sni = naiveSni;
    if (form.naiveEmail.trim()) settings.email = form.naiveEmail.trim();
    settings.tlsMode = form.naiveTlsMode;
    if (form.certFile.trim()) settings.certFile = form.certFile.trim();
    if (form.keyFile.trim()) settings.keyFile = form.keyFile.trim();
    settings.hideIp = form.naiveHideIp;
    settings.hideVia = form.naiveHideVia;
    settings.probeResistance = form.naiveProbeResistance;
    if (form.naiveFallbackRoot.trim()) settings.fallbackRoot = form.naiveFallbackRoot.trim();
    if (form.naiveWarpUpstream.trim()) settings.warpUpstream = form.naiveWarpUpstream.trim();
    else delete settings.warpUpstream;
  } else if (form.protocol === 'MIERU') {
    settings.transport = form.mieruTransport;
    if (form.mieruPortRange.trim()) settings.portRange = form.mieruPortRange.trim();
    else delete settings.portRange;
    settings.loggingLevel = form.mieruLoggingLevel;
    if (form.mieruDomain.trim()) settings.domain = form.mieruDomain.trim();
    else delete settings.domain;
    const mtu = parsePositiveNum(form.mieruMtu);
    if (mtu !== undefined && mtu >= 1280 && mtu <= 1500) settings.mtu = Math.floor(mtu);
    else delete settings.mtu;
    if (form.sni.trim()) settings.sni = form.sni.trim();
  } else if (form.protocol === 'TGWEB') {
    settings.domain = form.tgwebDomain.trim();
    settings.publicAddr = form.tgwebPublicAddr.trim();
    const basePath = form.tgwebBasePath.trim().replace(/^\/+|\/+$/g, '');
    if (basePath) settings.basePath = basePath;
    else delete settings.basePath;
    settings.carrier = form.tgwebCarrier;
    settings.secretMode = form.tgwebSecretMode;
    if (form.tgwebFallbackRoot.trim()) settings.fallbackRoot = form.tgwebFallbackRoot.trim();
  } else if (form.protocol === 'ANYTLS') {
    if (form.password.trim()) settings.password = form.password.trim();
    if (form.sni.trim()) settings.sni = form.sni.trim();
    settings.allowInsecure = form.allowInsecure;
    settings.alpn = alpnList;
    const paddingLines = form.anytlsPaddingScheme
      .split('\n')
      .map((s) => s.trim())
      .filter(Boolean);
    if (paddingLines.length) settings.paddingScheme = paddingLines;
    else delete settings.paddingScheme;
    if (form.anytlsIdleCheckInterval.trim()) {
      settings.idleSessionCheckInterval = form.anytlsIdleCheckInterval.trim();
    }
    if (form.anytlsIdleTimeout.trim()) {
      settings.idleSessionTimeout = form.anytlsIdleTimeout.trim();
    }
    const minIdle = parsePositiveNum(form.anytlsMinIdleSession);
    if (minIdle !== undefined) settings.minIdleSession = minIdle;
    if (form.certFile.trim()) settings.certFile = form.certFile.trim();
    if (form.keyFile.trim()) settings.keyFile = form.keyFile.trim();
  } else if (form.protocol === 'SHADOWTLS') {
    settings.version = Number(form.shadowtlsVersion) || 3;
    settings.handshakeServer = form.shadowtlsHandshakeServer.trim() || 'www.microsoft.com';
    settings.handshakePort = Math.max(1, Math.min(65535, Number(form.shadowtlsHandshakePort) || 443));
    settings.strictMode = form.shadowtlsStrictMode;
    if (form.shadowtlsDetour.trim()) settings.detour = form.shadowtlsDetour.trim();
    else delete settings.detour;
    if (form.shadowtlsSni.trim()) settings.sni = form.shadowtlsSni.trim();
    else if (form.sni.trim()) settings.sni = form.sni.trim();
    if (form.password.trim()) settings.password = form.password.trim();
  }

  if (isXrayProtocol(form.protocol)) {
    stream.network = form.network;
    stream.security = form.security;

    const domainsExcluded = form.sniffingDomainsExcluded
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    settings.sniffing = {
      enabled: form.sniffing,
      destOverride: form.sniffingDestOverride.length
        ? form.sniffingDestOverride
        : ['http', 'tls', 'quic', 'fakedns'],
      routeOnly: form.sniffingRouteOnly,
      ...(form.sniffingMetadataOnly ? { metadataOnly: true } : {}),
      ...(domainsExcluded.length ? { domainsExcluded } : {}),
    };

    if (form.sockoptEnabled) {
      // Preserve sockopt keys without explicit form fields (tcpMaxSeg, tcpUserTimeout,
      // tcpWindowClamp, penetrate, dialerProxy, ...) so saving an inbound never drops
      // tuning parameters captured from raw / imported configs.
      const existingSockopt = asObj(form.rawStream.sockopt ?? form.rawSettings.sockopt);
      const mark = parsePositiveNum(form.sockoptMark);
      const keepInt = parsePositiveNum(form.sockoptTcpKeepAliveInterval);
      const keepIdle = parsePositiveNum(form.sockoptTcpKeepAliveIdle);
      const sockopt: Record<string, unknown> = {
        ...existingSockopt,
        mark: mark ?? 0,
        V6Only: form.sockoptV6Only,
        tproxy: form.sockoptTproxy,
        tcpFastOpen: form.sockoptTcpFastOpen,
        tcpMptcp: form.sockoptTcpMptcp,
        ...(keepInt !== undefined ? { tcpKeepAliveInterval: keepInt } : {}),
        ...(keepIdle !== undefined ? { tcpKeepAliveIdle: keepIdle } : {}),
        ...(form.sockoptTcpCongestion.trim() ? { tcpcongestion: form.sockoptTcpCongestion.trim() } : {}),
        ...(form.sockoptInterface.trim() ? { interface: form.sockoptInterface.trim() } : {}),
        ...(form.sockoptAcceptProxyProtocol ? { acceptProxyProtocol: true } : {}),
        ...(form.sockoptDomainStrategy.trim() ? { domainStrategy: form.sockoptDomainStrategy.trim() } : {}),
      };
      // Xray's real key is lowercase `tcpcongestion`; drop any legacy camelCase alias.
      delete sockopt.tcpCongestion;
      if (!form.sockoptTcpCongestion.trim()) delete sockopt.tcpcongestion;
      stream.sockopt = sockopt;
      delete settings.sockopt;
    } else {
      delete stream.sockopt;
      delete settings.sockopt;
    }

    if (form.network === 'tcp' || form.network === 'raw') {
      const targetKey = form.network === 'raw' ? 'rawSettings' : 'tcpSettings';
      const otherKey = form.network === 'raw' ? 'tcpSettings' : 'rawSettings';
      delete stream[otherKey];
      if (form.tcpHeaderType === 'http' || form.tcpAcceptProxy) {
        stream[targetKey] = {
          ...(form.tcpAcceptProxy ? { acceptProxyProtocol: true } : {}),
          ...(form.tcpHeaderType === 'http'
            ? {
                header: {
                  type: 'http',
                  request: {
                    path: [form.tcpHeaderPath.trim() || '/'],
                    headers: form.tcpHeaderHost.trim()
                      ? { Host: [form.tcpHeaderHost.trim()] }
                      : {},
                  },
                },
              }
            : {}),
        };
      } else {
        delete stream[targetKey];
      }
    } else if (form.network === 'ws') {
      const extraHeaders = parseJsonSafe(form.wsHeadersJson) ?? {};
      const wsHost = form.wsHost.trim();
      const hb = parsePositiveNum(form.wsHeartbeatPeriod);
      stream.wsSettings = {
        path: form.wsPath.trim() || '/',
        ...(wsHost ? { host: wsHost } : {}),
        headers: wsHost ? { ...extraHeaders, Host: wsHost } : extraHeaders,
        ...(form.wsAcceptProxy ? { acceptProxyProtocol: true } : {}),
        ...(hb !== undefined && hb > 0 ? { heartbeatPeriod: hb } : {}),
      };
    } else if (form.network === 'grpc') {
      const idleTo = parsePositiveNum(form.grpcIdleTimeout);
      const hcTo = parsePositiveNum(form.grpcHealthCheckTimeout);
      const initWin = parsePositiveNum(form.grpcInitialWindowsSize);
      stream.grpcSettings = {
        serviceName: form.grpcServiceName.trim(),
        multiMode: form.grpcMultiMode,
        ...(form.grpcAuthority.trim() ? { authority: form.grpcAuthority.trim() } : {}),
        ...(idleTo !== undefined ? { idle_timeout: idleTo } : {}),
        ...(hcTo !== undefined ? { health_check_timeout: hcTo } : {}),
        ...(form.grpcPermitWithoutStream ? { permit_without_stream: true } : {}),
        ...(initWin !== undefined ? { initial_windows_size: initWin } : {}),
      };
    } else if (form.network === 'httpupgrade') {
      stream.httpupgradeSettings = {
        path: form.httpupgradePath.trim() || '/',
        host: form.httpupgradeHost.trim(),
        acceptProxyProtocol: form.httpupgradeAcceptProxy,
      };
    } else if (form.network === 'xhttp') {
      const baseExtra = parseJsonSafe(form.xhttpExtraJson) ?? {};
      const xmux: Record<string, unknown> = {};
      const xmuxFields: Array<[string, string]> = [
        ['maxConcurrency', form.xhttpMaxConcurrency],
        ['maxConnections', form.xhttpMaxConnections],
        ['cMaxReuseTimes', form.xhttpCMaxReuseTimes],
        ['hMaxRequestTimes', form.xhttpHMaxRequestTimes],
        ['hMaxReusableSecs', form.xhttpHMaxReusableSecs],
        ['hKeepAlivePeriod', form.xhttpHKeepAlivePeriod],
      ];
      for (const [k, raw] of xmuxFields) {
        if (raw.trim()) {
          const n = Number(raw);
          xmux[k] = Number.isFinite(n) ? n : raw.trim();
        }
      }
      const extraOut: Record<string, unknown> = { ...baseExtra };
      if (Object.keys(xmux).length) extraOut.xmux = xmux;
      const maxPost = parsePositiveNum(form.xhttpScMaxEachPostBytes);
      const minPosts = parsePositiveNum(form.xhttpScMinPostsIntervalMs);
      const maxBuf = parsePositiveNum(form.xhttpScMaxBufferedPosts);
      if (maxPost !== undefined) extraOut.scMaxEachPostBytes = maxPost;
      if (minPosts !== undefined) extraOut.scMinPostsIntervalMs = minPosts;
      if (maxBuf !== undefined) extraOut.scMaxBufferedPosts = maxBuf;
      if (form.xhttpScStreamUpServerSecs.trim()) {
        const n = Number(form.xhttpScStreamUpServerSecs.trim());
        extraOut.scStreamUpServerSecs = Number.isFinite(n) ? n : form.xhttpScStreamUpServerSecs.trim();
      }
      if (form.xhttpXPaddingBytes.trim()) {
        const n = Number(form.xhttpXPaddingBytes.trim());
        extraOut.xPaddingBytes = Number.isFinite(n) ? n : form.xhttpXPaddingBytes.trim();
      }
      if (form.xhttpNoGrpcHeader) extraOut.noGRPCHeader = true;
      if (form.xhttpNoSseHeader) extraOut.noSSEHeader = true;
      const xHeaders = parseJsonSafe(form.xhttpHeadersJson);
      if (xHeaders) extraOut.headers = xHeaders;
      const dl = parseJsonSafe(form.xhttpDownloadSettingsJson);
      if (dl) extraOut.downloadSettings = dl;

      stream.xhttpSettings = {
        path: form.xhttpPath.trim() || '/',
        host: form.xhttpHost.trim(),
        mode: form.xhttpMode,
        ...(maxPost !== undefined ? { scMaxEachPostBytes: maxPost } : {}),
        ...(minPosts !== undefined ? { scMinPostsIntervalMs: minPosts } : {}),
        ...(maxBuf !== undefined ? { scMaxBufferedPosts: maxBuf } : {}),
        ...(extraOut.scStreamUpServerSecs !== undefined ? { scStreamUpServerSecs: extraOut.scStreamUpServerSecs } : {}),
        ...(extraOut.xPaddingBytes !== undefined ? { xPaddingBytes: extraOut.xPaddingBytes } : {}),
        ...(form.xhttpNoGrpcHeader ? { noGRPCHeader: true } : {}),
        ...(form.xhttpNoSseHeader ? { noSSEHeader: true } : {}),
        ...(xHeaders ? { headers: xHeaders } : {}),
        ...(Object.keys(xmux).length ? { xmux } : {}),
        ...(dl ? { downloadSettings: dl } : {}),
        ...(Object.keys(extraOut).length ? { extra: extraOut } : {}),
      };
    } else if (form.network === 'h2') {
      stream.httpSettings = {
        path: form.h2Path.trim() || '/',
        host: form.h2Host
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
      };
    } else if (form.network === 'kcp') {
      const mtu = parsePositiveNum(form.kcpMtu);
      const tti = parsePositiveNum(form.kcpTti);
      const up = parsePositiveNum(form.kcpUplinkCapacity);
      const down = parsePositiveNum(form.kcpDownlinkCapacity);
      const rbuf = parsePositiveNum(form.kcpReadBufferSize);
      const wbuf = parsePositiveNum(form.kcpWriteBufferSize);
      stream.kcpSettings = {
        headerType: form.kcpHeaderType,
        seed: form.kcpSeed.trim(),
        ...(mtu !== undefined ? { mtu } : {}),
        ...(tti !== undefined ? { tti } : {}),
        ...(up !== undefined ? { uplinkCapacity: up } : {}),
        ...(down !== undefined ? { downlinkCapacity: down } : {}),
        ...(form.kcpCongestion ? { congestion: true } : {}),
        ...(rbuf !== undefined ? { readBufferSize: rbuf } : {}),
        ...(wbuf !== undefined ? { writeBufferSize: wbuf } : {}),
      };
    }

    if (form.security === 'tls') {
      const pinned = form.pinnedPeerCertSha256
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean);
      const verifyNames = form.verifyPeerCertByName
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean);

      stream.sni = form.sni.trim();
      stream.fingerprint = form.fingerprint.trim() || 'chrome';
      stream.alpn = alpnList;
      stream.allowInsecure = form.allowInsecure;
      if (pinned.length) stream.pinnedPeerCertSha256 = pinned.join(',');
      else delete stream.pinnedPeerCertSha256;
      if (verifyNames.length) stream.verifyPeerCertByName = verifyNames.join(',');
      else delete stream.verifyPeerCertByName;

      const existingTls = asObj(stream.tlsSettings);
      const certificates =
        form.certFile.trim() && form.keyFile.trim()
          ? [{ certificateFile: form.certFile.trim(), keyFile: form.keyFile.trim() }]
          : existingTls.certificates;
      const ocsp = parsePositiveNum(form.tlsOcspStapling);
      const curves = form.tlsCurvePreferences
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean);

      stream.tlsSettings = {
        ...existingTls,
        serverName: form.sni.trim(),
        fingerprint: form.fingerprint.trim() || 'chrome',
        alpn: alpnList,
        allowInsecure: form.allowInsecure,
        rejectUnknownSni: form.tlsRejectUnknownSni,
        ...(certificates ? { certificates } : {}),
        ...(pinned.length ? { pinnedPeerCertSha256: pinned.join(',') } : {}),
        ...(verifyNames.length ? { verifyPeerCertByName: verifyNames.join(',') } : {}),
        ...(form.echEnabled && form.echConfig.trim() ? { echConfig: form.echConfig.trim() } : {}),
        ...(form.echEnabled && form.echKeyPath.trim() ? { echKeyPath: form.echKeyPath.trim() } : {}),
        ...(form.echEnabled && form.tlsEchServerKeys.trim() ? { echServerKeys: form.tlsEchServerKeys.trim() } : {}),
        ...(form.tlsCipherSuites.trim() ? { cipherSuites: form.tlsCipherSuites.trim() } : {}),
        ...(curves.length ? { curvePreferences: curves } : {}),
        ...(ocsp !== undefined ? { ocspStapling: ocsp } : {}),
      };
    } else if (form.security === 'reality') {
      // Filter out empty strings so realityServerNames never becomes [""]
      const serverNames = form.realityServerNames
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean);
      // An empty field keeps the Remnawave-compatible `[""]` (admit clients with an
      // empty short ID); a non-empty list drops stray blank entries.
      const shortIds = form.realityShortIds.trim()
        ? form.realityShortIds
            .split(',')
            .map((s) => s.trim())
            .filter(Boolean)
        : [''];

      const dest = form.realityDest.trim() || 'www.google.com:443';
      const spiderX = form.realitySpiderX.trim() || '/';
      const fp = form.fingerprint.trim() || 'chrome';
      const primarySni = serverNames[0] || form.sni.trim();
      const xver = parsePositiveNum(form.realityXver) ?? 0;
      const maxTimeDiff = parsePositiveNum(form.realityMaxTimeDiff);
      const limitUp = parseJsonSafe(form.realityLimitFallbackUploadJson);
      const limitDown = parseJsonSafe(form.realityLimitFallbackDownloadJson);
      const privateKey = form.realityPrivateKey.trim();
      // Keep the keypair complete: derive `publicKey` from `privateKey` when it is
      // missing (raw imports) so `/sub/:token` always emits `pbk`.
      const publicKey =
        form.realityPublicKey.trim() ||
        (privateKey
          ? (() => {
              try {
                return toBase64Url(x25519PublicFromPrivate(fromBase64Url(privateKey)));
              } catch {
                return '';
              }
            })()
          : '');

      stream.sni = primarySni;
      stream.fingerprint = fp;
      if (alpnList.length) stream.alpn = alpnList;
      else delete stream.alpn;
      stream.publicKey = publicKey;
      stream.privateKey = privateKey;
      stream.shortId = shortIds[0] || '';
      stream.spiderX = spiderX;
      stream.dest = dest;
      stream.serverNames = serverNames;
      stream.shortIds = shortIds;
      if (form.mldsa65Seed.trim()) stream.mldsa65Seed = form.mldsa65Seed.trim();
      else delete stream.mldsa65Seed;
      if (form.mldsa65Verify.trim()) stream.mldsa65Verify = form.mldsa65Verify.trim();
      else delete stream.mldsa65Verify;

      const existingReality = asObj(stream.realitySettings);
      stream.realitySettings = {
        ...existingReality,
        dest,
        target: dest,
        xver,
        show: form.realityShow,
        privateKey,
        publicKey,
        serverNames,
        shortIds,
        spiderX,
        fingerprint: fp,
        ...(maxTimeDiff !== undefined ? { maxTimeDiff } : {}),
        ...(form.realityMinClientVer.trim() ? { minClientVer: form.realityMinClientVer.trim() } : {}),
        ...(form.realityMaxClientVer.trim() ? { maxClientVer: form.realityMaxClientVer.trim() } : {}),
        ...(limitUp ? { limitFallbackUpload: limitUp } : {}),
        ...(limitDown ? { limitFallbackDownload: limitDown } : {}),
        ...(form.mldsa65Seed.trim() ? { mldsa65Seed: form.mldsa65Seed.trim() } : {}),
        ...(form.mldsa65Verify.trim() ? { mldsa65Verify: form.mldsa65Verify.trim() } : {}),
      };
    }

    if (form.finalMaskEnabled) {
      const fm = parseJsonSafe(form.finalMaskJson);
      if (fm) stream.finalMask = fm;
    } else {
      delete stream.finalMask;
    }
  } else {
    // Non-Xray protocols (Sing-box / Naive / Mieru): preserve sni/allowInsecure on stream too for convenience
    if (form.sni.trim()) stream.sni = form.sni.trim();
    stream.allowInsecure = form.allowInsecure;
  }

  return {
    nodeId: form.nodeId,
    protocol: form.protocol,
    tag: form.tag.trim(),
    port: Math.max(1, Math.min(65535, Math.floor(Number(form.port) || 443))),
    listen: form.listen.trim() || '0.0.0.0',
    settings,
    stream,
    routing,
    sniffing: form.sniffing,
    remark: form.remark.trim() || null,
    enable: form.enable,
  };
}

export function applyRawInboundJsonToForm(
  rawJson: string,
  current: InboundFormState,
): { form: InboundFormState; error?: string } {
  try {
    const parsed = JSON.parse(rawJson) as Record<string, unknown>;
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return { form: current, error: 'Inbound configuration must be a JSON object' };
    }
    const stream = parsed.stream ?? parsed.streamSettings ?? {};
    const synthetic: Inbound = {
      id: current.id ?? '',
      nodeId: typeof parsed.nodeId === 'string' ? parsed.nodeId : current.nodeId,
      protocol: (typeof parsed.protocol === 'string'
        ? parsed.protocol.toUpperCase()
        : current.protocol) as Protocol,
      tag: typeof parsed.tag === 'string' ? parsed.tag : current.tag,
      port: typeof parsed.port === 'number' ? parsed.port : Number(current.port) || 443,
      listen: typeof parsed.listen === 'string' ? parsed.listen : current.listen,
      settings: asObj(parsed.settings),
      stream: asObj(stream),
      routing: asObj(parsed.routing),
      sniffing:
        typeof parsed.sniffing === 'boolean'
          ? parsed.sniffing
          : typeof (parsed.sniffing as Record<string, unknown> | undefined)?.enabled === 'boolean'
            ? Boolean((parsed.sniffing as Record<string, unknown>).enabled)
            : current.sniffing,
      remark: typeof parsed.remark === 'string' ? parsed.remark : current.remark,
      enable: typeof parsed.enable === 'boolean' ? parsed.enable : current.enable,
      createdAt: '',
      portShares: current.portShares as unknown as Inbound['portShares'],
    };
    const next = inboundToForm(synthetic);
    next.portShares = current.portShares;
    return { form: next };
  } catch (err) {
    return { form: current, error: err instanceof Error ? err.message : 'Invalid JSON' };
  }
}
