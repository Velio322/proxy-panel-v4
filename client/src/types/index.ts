export type UserRole = 'SUPER_ADMIN' | 'ADMIN' | 'RESELLER' | 'OPERATOR';
export type Language = 'en' | 'ru';

export type Protocol =
  | 'VLESS'
  | 'VMESS'
  | 'TROJAN'
  | 'SHADOWSOCKS'
  | 'HYSTERIA2'
  | 'NAIVEPROXY'
  | 'MIERU'
  | 'TUIC'
  | 'ANYTLS'
  | 'SHADOWTLS'
  | 'TGWEB';

export type TrafficResetStrategy =
  | 'NO_RESET'
  | 'DAY'
  | 'WEEK'
  | 'MONTH'
  | 'MONTH_ROLLING'
  | 'ON_HOLD';

export type NodeStatus = 'ONLINE' | 'OFFLINE' | 'ERROR' | 'MAINTENANCE';
export type PlanType = 'USER' | 'RESELLER';
export type SubTemplateType =
  | 'XRAY_JSON'
  | 'XRAY_BASE64'
  | 'MIHOMO'
  | 'CLASH'
  | 'STASH'
  | 'SINGBOX';

export type SrrAction =
  | 'ALLOW'
  | 'USE_TEMPLATE'
  | 'RESPOND_WITH_REMARKS'
  | 'ENCRYPT_AGE'
  | 'BLOCK_404'
  | 'BLOCK_451'
  | 'SOCKET_DROP';

export type NotificationType =
  | 'INFO'
  | 'WARNING'
  | 'ERROR'
  | 'SUCCESS'
  | 'TRAFFIC_LIMIT'
  | 'SUBSCRIPTION_EXPIRED'
  | 'NODE_OFFLINE'
  | 'PAYMENT_RECEIVED';

export interface User {
  id: string;
  username: string;
  email: string | null;
  role: UserRole;
  resellerId: string | null;
  language: Language | string;
  banned?: boolean;
  totpEnabled?: boolean;
  telegramId?: number | string | null;
  lastLoginAt?: string | null;
  lastLoginIp?: string | null;
  createdAt?: string;
  updatedAt?: string;
}

export interface LoginResult {
  user?: User;
  requires2fa?: boolean;
  totpRequired?: boolean;
  challengeToken?: string;
  tempToken?: string;
}

export interface BrandingConfig {
  panelName: string;
  hasCustomLogo: boolean;
  logoMime?: string | null;
  logoVersion?: string | null;
  logoUrl?: string | null;
}

export interface PublicConfig {
  apiUrl: string;
  subUrl: string;
  version?: string;
  telegramBotUsername?: string | null;
  branding?: BrandingConfig;
}

export interface NotificationItem {
  id: string;
  type: NotificationType;
  title: string;
  message: string;
  read: boolean;
  metadata?: Record<string, unknown> | null;
  createdAt: string;
}

export interface ApiKeyItem {
  id: string;
  name: string;
  permissions: string[];
  expiresAt: string | null;
  lastUsedAt: string | null;
  createdAt: string;
  key?: string;
}

export interface PasskeyCredential {
  id: string;
  name?: string | null;
  createdAt: string;
  lastUsedAt?: string | null;
}

export interface TelegramOAuthPayload {
  id: number;
  first_name?: string;
  last_name?: string;
  username?: string;
  photo_url?: string;
  auth_date: number;
  hash: string;
}

export interface DeviceItem {
  id: string;
  clientId: string;
  hwid: string;
  os?: string | null;
  osVersion?: string | null;
  model?: string | null;
  userAgent?: string | null;
  ip?: string | null;
  lastSeenAt: string;
  createdAt: string;
  updatedAt?: string;
}

export interface ClientNodeSummary {
  id: string;
  name: string;
  host: string;
  status?: NodeStatus;
  country?: string | null;
}

export interface ClientNodesResponse {
  allNodes: boolean;
  nodeIds: string[];
  nodes: ClientNodeSummary[];
}

export interface Client {
  id: string;
  uuid: string;
  username: string;
  email: string | null;
  trafficLimit: number;
  usedTraffic: number;
  uploadTraffic: number;
  downloadTraffic: number;
  expireAt: string | null;
  banned: boolean;
  restrictionMode?: 'SOFT' | 'HARD' | string;
  restrictionReason?: string | null;
  restrictedAt?: string | null;
  trafficResetDay?: number | null;
  resetStrategy?: TrafficResetStrategy;
  nextTrafficResetAt?: string | null;
  lastTrafficResetAt?: string | null;
  onHold?: boolean;
  activatedAt?: string | null;
  accessRevision?: number;
  note?: string | null;
  telegramId?: string | number | null;
  subId?: string | null;
  subToken: string;
  protocols: Protocol[];
  allNodes?: boolean;
  nodeIds?: string[];
  nodes?: ClientNodeSummary[];
  planId?: string | null;
  deviceLimit?: number | null;
  lastActiveAt?: string | null;
  lastLoginIp?: string | null;
  resellerId?: string | null;
  reseller?: { id: string; name: string } | null;
  plan?: { id: string; name: string; hwidLimit?: number | null } | null;
  settings?: { subId?: string; subPath?: string } | null;
  _count?: { devices?: number; inbounds?: number };
  createdAt: string;
  updatedAt?: string;
}

export interface ClientLinkEntry {
  protocol: Protocol;
  tag: string;
  host: string;
  port: number;
  link: string;
  rawUri: string;
  config: Record<string, unknown> | null;
}

export interface ClientLinksResponse {
  client: {
    username: string;
    uuid: string;
    upload: number;
    download: number;
    total: number;
    expire: number;
  };
  proxies: Array<string | Record<string, unknown>>;
  links: string[];
  entries: ClientLinkEntry[];
}

export interface PortShare {
  id?: string;
  inboundId?: string;
  protocol: Protocol;
  tag: string;
  host?: string | null;
  path?: string | null;
  settings: Record<string, unknown>;
  stream: Record<string, unknown>;
  enable: boolean;
  createdAt?: string;
  updatedAt?: string;
}

export interface Inbound {
  id: string;
  nodeId: string;
  protocol: Protocol;
  tag: string;
  port: number;
  listen: string;
  settings: Record<string, unknown>;
  stream: Record<string, unknown>;
  routing?: {
    blockTorrent?: boolean;
    blockAds?: boolean;
    customRules?: unknown[];
    [key: string]: unknown;
  } | null;
  sniffing: boolean;
  remark?: string | null;
  enable: boolean;
  portShares?: PortShare[];
  node?: { id: string; name: string; host: string; status: NodeStatus } | null;
  _count?: { clients?: number };
  createdAt?: string;
  updatedAt?: string;
}

export interface NodeMetric {
  cpuUsage: number;
  memUsage: number;
  tcpConnections: number;
  udpSockets: number;
  uploadBps: number;
  downloadBps: number;
  uptime?: number | null;
  recordedAt: string;
}

export interface NodeItem {
  id: string;
  name: string;
  host: string;
  port: number;
  apiPort: number;
  status: NodeStatus;
  country?: string | null;
  city?: string | null;
  isp?: string | null;
  lastCheckAt?: string | null;
  lastPingMs?: number | null;
  uptime?: number | null;
  version?: string | null;
  cpuUsage?: number | null;
  memUsage?: number | null;
  netUpload?: number | null;
  netDownload?: number | null;
  xrayVersion?: string | null;
  singboxVersion?: string | null;
  naiveVersion?: string | null;
  mieruVersion?: string | null;
  telemtVersion?: string | null;
  arch?: string | null;
  features?: Record<string, unknown> | null;
  tags: string[];
  active: boolean;
  mtlsEnabled?: boolean;
  mtlsFingerprint?: string | null;
  inbounds?: Array<{ id: string; protocol: Protocol; tag: string; enable: boolean; port: number }>;
  nodeMetrics?: NodeMetric[];
  _count?: { inbounds?: number };
  setupInstructions?: string;
  installCommand?: string;
  createdAt: string;
  updatedAt?: string;
}

export interface RoutingRule {
  id: string;
  name: string;
  description?: string | null;
  enabled: boolean;
  priority: number;
  type: 'field' | 'logical';
  domain?: string[];
  ip?: string[];
  port?: string | null;
  sourcePort?: string | null;
  source?: string[];
  protocol?: string[];
  inboundTag?: string[];
  outboundTag: string;
  balancerTag?: string | null;
  ruleSet?: string[];
  subRules?: Record<string, unknown> | null;
  nodeScope?: string | null;
  createdAt?: string;
  updatedAt?: string;
}

export interface Plan {
  id: string;
  name: string;
  description?: string | null;
  type: PlanType;
  price: number;
  currency: string;
  duration: number;
  periodDays?: number;
  trafficLimit: number;
  trafficLimitBytes?: number;
  maxClients: number;
  deviceLimit?: number;
  hwidLimit?: number | null;
  maxSpeed: number;
  speedLimitMbps?: number;
  defaultResetStrategy?: TrafficResetStrategy;
  protocols: Protocol[];
  features?: string[];
  nodeIds?: string[];
  sortOrder?: number;
  active: boolean;
  _count?: { clients?: number; subscriptions?: number };
  createdAt?: string;
  updatedAt?: string;
}

export interface Reseller {
  id: string;
  name: string;
  contactEmail?: string | null;
  contactTelegram?: string | null;
  maxClients: number;
  maxNodes: number;
  trafficLimit: number;
  usedTraffic: number;
  balance: number;
  whiteLabel?: Record<string, unknown> | null;
  active: boolean;
  expiresAt?: string | null;
  createdAt?: string;
  updatedAt?: string;
}

export interface HostOverride {
  id: string;
  inboundId: string;
  remark: string;
  address: string;
  port?: number | null;
  sni?: string | null;
  host?: string | null;
  path?: string | null;
  alpn?: string | null;
  fingerprint?: string | null;
  isCdn?: boolean;
  shuffleHost?: boolean;
  enable: boolean;
  tags?: string[];
  allowInsecure?: boolean | null;
  pinnedPeerCertSha256?: string | null;
  verifyPeerCertByName?: string | null;
  mihomoIpVersion?: string | null;
  serverDescription?: string | null;
  xhttpExtra?: Record<string, unknown> | null;
  finalMask?: Record<string, unknown> | null;
  vlessRouteId?: string | null;
  fragment?: { packets?: string; length?: string; interval?: string } | null;
  sortOrder?: number;
  inbound?: { id: string; tag: string; protocol: Protocol; port: number; node?: { name: string } | null };
  createdAt?: string;
  updatedAt?: string;
}

export interface XrayValidationResult {
  valid: boolean;
  timestamp: string;
  message: string;
  errors: string[];
  warnings: string[];
  engine: string;
  xrayVersion?: string | null;
}

export interface XrayNodeProfile {
  nodeId: string;
  nodeName: string;
  profileName: string;
  config: Record<string, unknown>;
  savedProfile?: Record<string, unknown> | null;
  updatedAt?: string;
}

export interface WebhookItem {
  id: string;
  name: string;
  url: string;
  secret?: string;
  scopes: string[];
  events: string[];
  active: boolean;
  lastStatus?: number | null;
  lastError?: string | null;
  lastDeliveredAt?: string | null;
  createdAt?: string;
  updatedAt?: string;
}

export interface SubscriptionTemplate {
  id: string;
  name: string;
  templateType: SubTemplateType;
  content: string;
  isDefault: boolean;
  description?: string | null;
  createdAt?: string;
  updatedAt?: string;
}

export interface SrrRule {
  id: string;
  name: string;
  enabled: boolean;
  priority: number;
  matchHeader: string;
  matchPattern: string;
  action: SrrAction;
  templateId?: string | null;
  customRemarks?: string | null;
  agePublicKey?: string | null;
  createdAt?: string;
  updatedAt?: string;
}

export interface SessionEntry {
  id: string;
  nodeId: string;
  nodeName?: string;
  clientId?: string | null;
  username?: string | null;
  protocol?: string | null;
  inboundTag?: string | null;
  sourceIp: string;
  target?: string | null;
  uploadBytes?: number;
  downloadBytes?: number;
  connectedAt: string;
}

export interface NodeLogEntry {
  timestamp: string;
  core: string;
  level: string;
  stream?: 'stdout' | 'stderr' | string;
  message: string;
}

export interface SrhEntry {
  id: string;
  clientId?: string | null;
  username?: string | null;
  ip: string;
  userAgent: string;
  detectedApp?: string | null;
  format?: string | null;
  srrRuleName?: string | null;
  status: number;
  hwid?: string | null;
  createdAt: string;
}

export interface TorrentBlockerReport {
  id: string;
  nodeId?: string | null;
  nodeName?: string | null;
  clientId?: string | null;
  username?: string | null;
  sourceIp: string;
  protocol?: string | null;
  rule?: string | null;
  action?: string | null;
  createdAt: string;
}

export interface InfraProvider {
  id: string;
  name: string;
  website?: string | null;
  supportUrl?: string | null;
  notes?: string | null;
  createdAt?: string;
}

export interface InfraNodeBilling {
  id: string;
  nodeId: string;
  nodeName?: string;
  providerId: string;
  providerName?: string;
  amount: number;
  currency: string;
  billingCycle: 'MONTHLY' | 'QUARTERLY' | 'YEARLY';
  nextDueDate: string;
  autoRenew: boolean;
  notes?: string | null;
}

export interface AuditLog {
  id: string;
  action: string;
  resource: string;
  resourceId?: string | null;
  details?: Record<string, unknown> | null;
  ip?: string | null;
  userAgent?: string | null;
  createdAt: string;
  user?: { id: string; username: string; role: UserRole } | null;
}

export interface BackupLog {
  id: string;
  type: 'DATABASE' | 'CONFIG' | 'FULL' | string;
  destination: 'local' | 'telegram' | 's3' | string;
  status: 'PENDING' | 'IN_PROGRESS' | 'SUCCESS' | 'COMPLETED' | 'FAILED' | string;
  filename?: string | null;
  filePath?: string | null;
  sizeBytes?: number | null;
  fileSize?: number | null;
  error?: string | null;
  createdAt: string;
}

export interface DashboardOverview {
  clients: { total: number; active: number; banned: number };
  nodes: { total: number; online: number; offline: number };
  inbounds: { total: number };
  expiringToday: number;
  traffic: {
    today: { upload: number; download: number };
    month: { upload: number; download: number };
  };
  onlineUsers?: number;
}

export interface TrafficChartPoint {
  day: string;
  upload: number;
  download: number;
}

export interface TopClient {
  clientId: string;
  _sum: { upload: number; download: number };
  client: { id: string; username: string; uuid: string };
}

export interface HostTelemetry {
  sampledAt: string;
  cpu: {
    count?: number;
    usagePercent: number;
    loadAverage?: [number, number, number];
  };
  memory: {
    usagePercent: number;
    totalBytes?: number;
    usedBytes?: number;
    freeBytes?: number;
  };
  uptime: { hostSeconds: number; processSeconds: number };
  connections: {
    establishedTcp: number;
    udpSockets: number;
    proxyUserCount?: number;
  };
  network: {
    uploadBps: number;
    downloadBps: number;
    receivedBytes?: number;
    transmittedBytes?: number;
  };
  process?: {
    pid?: number;
    memory?: { rss?: number };
  };
}

export interface NodeMonitoringMetrics {
  sampledAt?: string;
  status?: string;
  version?: string;
  uptime?: number;
  cpuUsage?: number;
  memUsage?: number;
  tcpConnections?: number;
  udpSockets?: number;
  uploadBps?: number;
  downloadBps?: number;
  onlineUsersCount?: number;
  xrayRunning?: boolean;
  singboxRunning?: boolean;
  naiveRunning?: boolean;
  mieruRunning?: boolean;
  xrayPid?: number | null;
  singboxPid?: number | null;
  naivePid?: number | null;
  mieruPid?: number | null;
  cores?: {
    arch?: string;
    xray?: { running?: boolean; version?: string | null };
    singbox?: { running?: boolean; version?: string | null };
    naive?: { running?: boolean; version?: string | null };
    mieru?: { running?: boolean; version?: string | null };
  };
  coreStatuses?: Record<string, string>;
  traffic?: {
    total?: { upload: number; download: number };
    perUser?: Record<string, { upload: number; download: number }>;
  };
  trafficCapabilities?: Array<{
    core: string;
    source: string;
    available: boolean;
    bytes?: string;
    sessionTermination?: boolean;
    mode?: string;
  }>;
  hostTelemetry?: HostTelemetry;
}

export interface CoreVersionEntry {
  core: 'xray' | 'singbox' | 'naive' | 'mieru';
  name?: string;
  latestVersion: string;
  tag?: string;
  arch?: string;
  sha256?: string;
  downloadUrl?: string;
  releaseUrl?: string;
  publishedAt?: string;
  error?: string;
}
