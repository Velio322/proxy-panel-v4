import type { Protocol } from '@/types';

export type NetworkType = 'tcp' | 'raw' | 'ws' | 'grpc' | 'httpupgrade' | 'xhttp' | 'h2' | 'kcp';
export type SecurityType = 'none' | 'tls' | 'reality';
export type VlessFlowType = 'none' | 'xtls-rprx-vision' | 'xtls-rprx-vision-udp443';
export type Hy2ObfsType = 'none' | 'salamander' | 'gecko';
export type Hy2BbrProfile = 'none' | 'conservative' | 'standard' | 'aggressive';

export interface PortShareFormState {
  id?: string;
  protocol: Protocol;
  tag: string;
  host: string;
  path: string;
  enable: boolean;
  settings: Record<string, unknown>;
  stream: Record<string, unknown>;
}

export interface InboundFormState {
  id?: string;
  nodeId: string;
  protocol: Protocol;
  tag: string;
  port: string;
  listen: string;
  remark: string;
  enable: boolean;
  sniffing: boolean;

  // Transport
  network: NetworkType;
  tcpHeaderType: 'none' | 'http';
  tcpHeaderHost: string;
  tcpHeaderPath: string;
  tcpAcceptProxy: boolean;
  wsPath: string;
  wsHost: string;
  wsHeadersJson: string;
  wsAcceptProxy: boolean;
  wsHeartbeatPeriod: string;
  grpcServiceName: string;
  grpcAuthority: string;
  grpcMultiMode: boolean;
  grpcIdleTimeout: string;
  grpcHealthCheckTimeout: string;
  grpcPermitWithoutStream: boolean;
  grpcInitialWindowsSize: string;
  httpupgradePath: string;
  httpupgradeHost: string;
  httpupgradeAcceptProxy: boolean;
  xhttpPath: string;
  xhttpHost: string;
  xhttpMode: 'auto' | 'packet-up' | 'stream-up' | 'stream-one';
  xhttpMaxConcurrency: string;
  xhttpMaxConnections: string;
  xhttpCMaxReuseTimes: string;
  xhttpHMaxRequestTimes: string;
  xhttpHMaxReusableSecs: string;
  xhttpHKeepAlivePeriod: string;
  xhttpScMaxEachPostBytes: string;
  xhttpScMinPostsIntervalMs: string;
  xhttpScMaxBufferedPosts: string;
  xhttpScStreamUpServerSecs: string;
  xhttpXPaddingBytes: string;
  xhttpNoGrpcHeader: boolean;
  xhttpNoSseHeader: boolean;
  xhttpHeadersJson: string;
  xhttpDownloadSettingsJson: string;
  xhttpExtraJson: string;
  h2Path: string;
  h2Host: string;
  kcpHeaderType: 'none' | 'srtp' | 'utp' | 'wechat-video' | 'dtls' | 'wireguard';
  kcpSeed: string;
  kcpMtu: string;
  kcpTti: string;
  kcpUplinkCapacity: string;
  kcpDownlinkCapacity: string;
  kcpCongestion: boolean;
  kcpReadBufferSize: string;
  kcpWriteBufferSize: string;

  // Sniffing & Sockopt (Xray 26)
  sniffingDestOverride: string[];
  sniffingRouteOnly: boolean;
  sniffingMetadataOnly: boolean;
  sniffingDomainsExcluded: string;
  sockoptEnabled: boolean;
  sockoptMark: string;
  sockoptV6Only: boolean;
  sockoptTproxy: 'off' | 'tproxy' | 'redirect';
  sockoptTcpFastOpen: boolean;
  sockoptTcpMptcp: boolean;
  sockoptTcpKeepAliveInterval: string;
  sockoptTcpKeepAliveIdle: string;
  sockoptTcpCongestion: string;
  sockoptInterface: string;
  sockoptAcceptProxyProtocol: boolean;
  sockoptDomainStrategy: string;

  // Security (TLS / Reality / ECH)
  security: SecurityType;
  sni: string;
  alpn: string;
  fingerprint: string;
  allowInsecure: boolean;
  certFile: string;
  keyFile: string;
  pinnedPeerCertSha256: string;
  verifyPeerCertByName: string;
  echEnabled: boolean;
  echConfig: string;
  echKeyPath: string;
  tlsCipherSuites: string;
  tlsCurvePreferences: string;
  tlsOcspStapling: string;
  tlsEchServerKeys: string;
  tlsRejectUnknownSni: boolean;

  // Reality
  realityDest: string;
  realityServerNames: string;
  realityPrivateKey: string;
  realityPublicKey: string;
  realityShortIds: string;
  realitySpiderX: string;
  realityXver: string;
  realityShow: boolean;
  realityMaxTimeDiff: string;
  realityMinClientVer: string;
  realityMaxClientVer: string;
  realityLimitFallbackUploadJson: string;
  realityLimitFallbackDownloadJson: string;
  mldsa65Seed: string;
  mldsa65Verify: string;

  // FinalMask
  finalMaskEnabled: boolean;
  finalMaskJson: string;

  // VLESS & Trojan
  vlessFlow: VlessFlowType;
  vlessDecryption: string;
  vlessEncryption: string;
  vlessRoute: string;
  vlessFallbacksJson: string;
  trojanFallbacksJson: string;

  // VMess
  vmessCipher: 'auto' | 'aes-128-gcm' | 'chacha20-poly1305' | 'none' | 'zero';
  alterId: string;

  // Shadowsocks
  ssMethod:
    | '2022-blake3-aes-128-gcm'
    | '2022-blake3-aes-256-gcm'
    | '2022-blake3-chacha20-poly1305'
    | 'aes-256-gcm'
    | 'aes-128-gcm'
    | 'chacha20-ietf-poly1305';
  ssNetwork: 'tcp,udp' | 'tcp' | 'udp';

  // Common fallback credentials
  password: string;
  uuid: string;

  // Hysteria2
  hy2ObfsType: Hy2ObfsType;
  hy2ObfsPassword: string;
  hy2GeckoMinPacketSize: string;
  hy2GeckoMaxPacketSize: string;
  hy2BandwidthUp: string;
  hy2BandwidthDown: string;
  hy2MaxClient: string;
  hy2MaxStream: string;
  hy2IgnoreClientBandwidth: boolean;
  hy2Masquerade: string;
  hy2PortRange: string;
  hy2BbrProfile: Hy2BbrProfile;
  disableChromeParrot: boolean;

  // TUIC
  tuicCongestionControl: 'bbr' | 'cubic' | 'new_reno';
  tuicZeroRtt: boolean;
  tuicAuthTimeout: string;
  tuicHeartbeat: string;
  tuicUdpRelayMode: 'native' | 'quic';

  // NaiveProxy
  naiveProto: 'https' | 'quic';
  naiveDomain: string;
  naiveEmail: string;
  naiveTlsMode: 'letsencrypt' | 'custom';
  naiveHideIp: boolean;
  naiveHideVia: boolean;
  naiveProbeResistance: boolean;
  naiveFallbackRoot: string;
  naiveWarpUpstream: string;

  // Mieru
  mieruTransport: 'tcp' | 'udp' | 'both';
  mieruPortRange: string;
  mieruLoggingLevel: 'FATAL' | 'ERROR' | 'WARN' | 'INFO' | 'DEBUG' | 'TRACE';
  mieruDomain: string;

  // AnyTLS
  anytlsPaddingScheme: string;
  anytlsIdleCheckInterval: string;
  anytlsIdleTimeout: string;
  anytlsMinIdleSession: string;

  // ShadowTLS
  shadowtlsVersion: '1' | '2' | '3';
  shadowtlsHandshakeServer: string;
  shadowtlsHandshakePort: string;
  shadowtlsStrictMode: boolean;
  shadowtlsDetour: string;
  shadowtlsSni: string;

  // Routing
  routingBlockTorrent: boolean;
  routingBlockAds: boolean;

  // Port Shares
  portShares: PortShareFormState[];

  // Preserved unknown backend JSON keys
  rawSettings: Record<string, unknown>;
  rawStream: Record<string, unknown>;
  rawRouting: Record<string, unknown>;
}
