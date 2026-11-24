import axios from 'axios';
import type { startAuthentication, startRegistration } from '@simplewebauthn/browser';

type PublicKeyCredentialCreationOptionsJSON = Parameters<typeof startRegistration>[0]['optionsJSON'];
type RegistrationResponseJSON = Awaited<ReturnType<typeof startRegistration>>;
type PublicKeyCredentialRequestOptionsJSON = Parameters<typeof startAuthentication>[0]['optionsJSON'];
type AuthenticationResponseJSON = Awaited<ReturnType<typeof startAuthentication>>;
import type {
  ApiKeyItem,
  AuditLog,
  BackupLog,
  BrandingConfig,
  Client,
  ClientLinksResponse,
  ClientNodesResponse,
  CoreVersionEntry,
  DashboardOverview,
  DeviceItem,
  HostOverride,
  HostTelemetry,
  Inbound,
  InfraNodeBilling,
  InfraProvider,
  LoginResult,
  NodeItem,
  NodeLogEntry,
  NodeMonitoringMetrics,
  NotificationItem,
  PasskeyCredential,
  Plan,
  PortShare,
  PublicConfig,
  Reseller,
  RoutingRule,
  SessionEntry,
  SrhEntry,
  SrrRule,
  SubscriptionTemplate,
  TelegramOAuthPayload,
  TopClient,
  TorrentBlockerReport,
  TrafficChartPoint,
  User,
  WebhookItem,
} from '@/types';

let onUnauthorizedCallback: (() => void) | null = null;

export function registerUnauthorizedHandler(cb: () => void) {
  onUnauthorizedCallback = cb;
}

// Cookie-only browser auth (`HttpOnly` auth_token). Zero localStorage JWT or Bearer injection.
export const api = axios.create({
  baseURL: '/api/v1',
  withCredentials: true,
  headers: {
    'Content-Type': 'application/json',
  },
});

api.interceptors.response.use(
  (res) => res,
  (err) => {
    const status = err?.response?.status;
    const reqUrl = String(err?.config?.url || '');
    const isAuthProbe =
      reqUrl.includes('/auth/me') ||
      reqUrl.includes('/auth/login') ||
      reqUrl.includes('/auth/login/2fa') ||
      reqUrl.includes('/auth/2fa/') ||
      reqUrl.includes('/auth/passkeys/login') ||
      reqUrl.includes('/auth/telegram/login') ||
      reqUrl.includes('/auth/telegram');

    if (status === 401 && !isAuthProbe) {
      onUnauthorizedCallback?.();
      if (typeof window !== 'undefined' && window.location.pathname !== '/login') {
        window.location.assign('/login');
      }
    }
    return Promise.reject(err);
  },
);

export function getApiErrorMessage(err: unknown, fallback = 'Request failed'): string {
  if (axios.isAxiosError(err)) {
    const data = err.response?.data as
      | {
          error?: string;
          message?: string;
          details?: Array<{ message?: string; path?: string[] }>;
        }
      | undefined;
    const detailMsg = Array.isArray(data?.details)
      ? data.details
          .map((d) => d?.message?.trim())
          .filter((m): m is string => Boolean(m))
          .join('; ')
      : '';
    if (detailMsg) {
      if (!data?.error || data.error === 'Validation error') {
        return detailMsg;
      }
      return `${data.error}: ${detailMsg}`;
    }
    return data?.error || data?.message || err.message || fallback;
  }
  return err instanceof Error ? err.message : fallback;
}

export const authApi = {
  login: async (payload: { username: string; password: string }): Promise<LoginResult> => {
    const { data } = await api.post<LoginResult>('/auth/login', payload);
    return data;
  },
  verify2fa: async (payload: { challengeToken: string; code: string }): Promise<{ user: User }> => {
    const { data } = await api.post<{ user: User }>('/auth/login/2fa', payload);
    return data;
  },
  me: async (): Promise<User> => {
    const { data } = await api.get<User>('/auth/me');
    return data;
  },
  logout: async (): Promise<{ success: boolean }> => {
    const { data } = await api.post<{ success: boolean }>('/auth/logout');
    return data;
  },
  changePassword: async (payload: {
    currentPassword: string;
    newPassword: string;
  }): Promise<{ success: boolean }> => {
    const { data } = await api.post<{ success: boolean }>('/auth/change-password', payload);
    return data;
  },
  setup2fa: async (): Promise<{ secret: string; otpauthUrl: string; qrSvg?: string }> => {
    const { data } = await api.post<{ secret: string; otpauthUrl: string; qrSvg?: string }>('/auth/2fa/setup');
    return data;
  },
  enable2fa: async (code: string): Promise<{ success: boolean }> => {
    const { data } = await api.post<{ success: boolean }>('/auth/2fa/enable', { code });
    return data;
  },
  disable2fa: async (payload: { code: string; password?: string }): Promise<{ success: boolean }> => {
    const { data } = await api.post<{ success: boolean }>('/auth/2fa/disable', payload);
    return data;
  },
  listPasskeys: async (): Promise<PasskeyCredential[]> => {
    const { data } = await api.get<PasskeyCredential[]>('/auth/passkeys');
    return Array.isArray(data) ? data : [];
  },
  startPasskeyRegistration: async (): Promise<{
    options: PublicKeyCredentialCreationOptionsJSON;
    challengeId: string;
  }> => {
    const { data } = await api.post<{
      options: PublicKeyCredentialCreationOptionsJSON;
      challengeId: string;
    }>('/auth/passkeys/register/start');
    return data;
  },
  finishPasskeyRegistration: async (payload: {
    challengeId: string;
    name?: string;
    credential: RegistrationResponseJSON;
  }): Promise<PasskeyCredential> => {
    const { data } = await api.post<PasskeyCredential>('/auth/passkeys/register/finish', payload);
    return data;
  },
  deletePasskey: async (id: string): Promise<{ success: boolean }> => {
    const { data } = await api.delete<{ success: boolean }>(`/auth/passkeys/${id}`);
    return data;
  },
  startPasskeyLogin: async (
    username?: string,
  ): Promise<{ options: PublicKeyCredentialRequestOptionsJSON; challengeId: string }> => {
    const { data } = await api.post<{
      options: PublicKeyCredentialRequestOptionsJSON;
      challengeId: string;
    }>('/auth/passkeys/login/start', username ? { username } : {});
    return data;
  },
  finishPasskeyLogin: async (payload: {
    challengeId: string;
    credential: AuthenticationResponseJSON;
  }): Promise<{ user: User }> => {
    const { data } = await api.post<{ user: User }>('/auth/passkeys/login/finish', payload);
    return data;
  },
  telegramLogin: async (payload: TelegramOAuthPayload): Promise<{ user: User }> => {
    const { data } = await api.post<{ user: User }>('/auth/telegram/login', payload);
    return data;
  },
  linkTelegram: async (payload: TelegramOAuthPayload): Promise<{ user: User }> => {
    const { data } = await api.post<{ user: User }>('/auth/telegram/link', payload);
    return data;
  },
  unlinkTelegram: async (): Promise<{ success: boolean }> => {
    const { data } = await api.post<{ success: boolean }>('/auth/telegram/unlink');
    return data;
  },
};

export const publicConfigApi = {
  get: async (): Promise<PublicConfig> => {
    const { data } = await api.get<PublicConfig>('/public-config');
    return data;
  },
  health: async (): Promise<{ status: string; version?: string; timestamp?: string }> => {
    const { data } = await axios.get<{ status: string; version?: string; timestamp?: string }>('/api/health', {
      withCredentials: true,
    });
    return data;
  },
};

export const brandingApi = {
  get: async (): Promise<BrandingConfig> => {
    const { data } = await api.get<BrandingConfig>('/branding');
    return data;
  },
  update: async (payload: {
    panelName?: string;
    logoDataUrl?: string;
    svgCode?: string;
    clearLogo?: boolean;
  }): Promise<BrandingConfig> => {
    const { data } = await api.put<BrandingConfig>('/branding', payload);
    return data;
  },
  reset: async (): Promise<BrandingConfig> => {
    const { data } = await api.delete<BrandingConfig>('/branding');
    return data;
  },
};

export const notificationsApi = {
  list: async (): Promise<{ notifications: NotificationItem[]; unreadCount: number }> => {
    const { data } = await api.get<
      NotificationItem[] | { notifications?: NotificationItem[]; unreadCount?: number }
    >('/notifications');
    if (Array.isArray(data)) {
      return {
        notifications: data,
        unreadCount: data.filter((n) => !n.read).length,
      };
    }
    const items = Array.isArray(data?.notifications) ? data.notifications : [];
    const unread =
      typeof data?.unreadCount === 'number' ? data.unreadCount : items.filter((n) => !n.read).length;
    return { notifications: items, unreadCount: unread };
  },
  markRead: async (id: string): Promise<void> => {
    await api.post(`/notifications/${id}/read`);
  },
  markAllRead: async (): Promise<void> => {
    await api.post('/notifications/read-all');
  },
};

export const usersApi = {
  list: async (params?: { search?: string; role?: string; limit?: number }): Promise<{
    data: User[];
    total: number;
  }> => {
    const { data } = await api.get<{ data: User[]; total: number } | User[]>('/users', {
      params: { limit: 100, ...params },
    });
    if (Array.isArray(data)) return { data, total: data.length };
    return { data: data.data ?? [], total: data.total ?? 0 };
  },
  create: async (payload: Record<string, unknown>): Promise<User & { password?: string }> => {
    const { data } = await api.post<User & { password?: string }>('/users', payload);
    return data;
  },
  createReseller: async (payload: Record<string, unknown>): Promise<unknown> => {
    const { data } = await api.post('/users/create-reseller', payload);
    return data;
  },
  update: async (id: string, payload: Record<string, unknown>): Promise<User> => {
    const { data } = await api.put<User>(`/users/${id}`, payload);
    return data;
  },
  delete: async (id: string): Promise<void> => {
    await api.delete(`/users/${id}`);
  },
  resetPassword: async (id: string, password?: string): Promise<{ password: string }> => {
    const { data } = await api.post<{ password: string }>(`/users/${id}/reset-password`, password ? { password } : {});
    return data;
  },
  toggleBan: async (id: string): Promise<User> => {
    const { data } = await api.post<User>(`/users/${id}/toggle-ban`);
    return data;
  },
  // Real API Keys CRUD
  listApiKeys: async (userId: string): Promise<ApiKeyItem[]> => {
    const { data } = await api.get<ApiKeyItem[]>(`/users/${userId}/api-keys`);
    return Array.isArray(data) ? data : [];
  },
  createApiKey: async (
    userId: string,
    payload: { name: string; permissions: string[]; expiresAt?: string | null },
  ): Promise<ApiKeyItem> => {
    const { data } = await api.post<ApiKeyItem>(`/users/${userId}/api-keys`, payload);
    return data;
  },
  deleteApiKey: async (userId: string, keyId: string): Promise<void> => {
    await api.delete(`/users/${userId}/api-keys/${keyId}`);
  },
};

export const clientsApi = {
  list: async (params?: {
    search?: string;
    page?: number;
    limit?: number;
    banned?: string;
    protocol?: string;
    nodeId?: string;
    pinnedOnly?: boolean;
    sortBy?: string;
    sortOrder?: string;
  }): Promise<{ data: Client[]; total: number; page: number; pages: number; limit: number }> => {
    const { data } = await api.get<{
      data: Client[];
      total: number;
      page: number;
      pages: number;
      limit: number;
    }>('/clients', { params });
    return data;
  },
  get: async (id: string): Promise<Client> => {
    const { data } = await api.get<Client>(`/clients/${id}`);
    return data;
  },
  getLinks: async (id: string): Promise<ClientLinksResponse> => {
    const { data } = await api.get<ClientLinksResponse>(`/clients/${id}/links`);
    return data;
  },
  create: async (payload: Record<string, unknown>): Promise<Client & { password?: string }> => {
    const { data } = await api.post<Client & { password?: string }>('/clients', payload);
    return data;
  },
  update: async (id: string, payload: Record<string, unknown>): Promise<Client> => {
    const { data } = await api.put<Client>(`/clients/${id}`, payload);
    return data;
  },
  delete: async (id: string): Promise<void> => {
    await api.delete(`/clients/${id}`);
  },
  resetTraffic: async (id: string): Promise<void> => {
    await api.post(`/clients/${id}/reset-traffic`);
  },
  toggleBan: async (id: string): Promise<Client> => {
    const { data } = await api.post<Client>(`/clients/${id}/toggle-ban`);
    return data;
  },
  getInbounds: async (id: string): Promise<{ inboundIds: string[]; inbounds?: Inbound[] }> => {
    const { data } = await api.get<
      { inboundIds?: string[]; inbounds?: Inbound[] } | Array<string | { inboundId?: string; id: string }>
    >(`/clients/${id}/inbounds`);
    if (Array.isArray(data)) {
      const ids = data.map((item) =>
        typeof item === 'string' ? item : (item.inboundId ?? item.id),
      );
      return { inboundIds: ids };
    }
    return {
      inboundIds: data.inboundIds ?? (data.inbounds ? data.inbounds.map((i) => i.id) : []),
      inbounds: data.inbounds,
    };
  },
  updateInbounds: async (id: string, inboundIds: string[]): Promise<void> => {
    await api.put(`/clients/${id}/inbounds`, { inboundIds });
  },
  getNodes: async (id: string): Promise<ClientNodesResponse> => {
    const { data } = await api.get<ClientNodesResponse>(`/clients/${id}/nodes`);
    return data;
  },
  updateNodes: async (
    id: string,
    payload: { allNodes: boolean; nodeIds: string[]; syncInbounds?: boolean },
  ): Promise<ClientNodesResponse> => {
    const { data } = await api.put<ClientNodesResponse>(`/clients/${id}/nodes`, payload);
    return data;
  },
  listDevices: async (id: string): Promise<DeviceItem[]> => {
    const { data } = await api.get<DeviceItem[] | { devices?: DeviceItem[] }>(`/clients/${id}/devices`);
    return Array.isArray(data) ? data : data.devices ?? [];
  },
  deleteDevice: async (id: string, deviceId: string): Promise<void> => {
    await api.delete(`/clients/${id}/devices/${deviceId}`);
  },
  resetDevices: async (id: string): Promise<void> => {
    await api.delete(`/clients/${id}/devices`);
  },
  exportConfig: async (id: string, format: 'uci' | 'passwall' | 'openclash' | 'singbox' | 'ssr'): Promise<string> => {
    const { data } = await api.get<string | Record<string, unknown>>(`/clients/${id}/export/${format}`, {
      responseType: 'text',
      transformResponse: [(v) => v],
    });
    return typeof data === 'string' ? data : JSON.stringify(data, null, 2);
  },
  importCsv: async (payload: { csv: string } | { rows: unknown[] }): Promise<{
    created: number;
    skipped: number;
    errors?: string[];
  }> => {
    const { data } = await api.post<{ created: number; skipped: number; errors?: string[] }>(
      '/clients/import',
      payload,
    );
    return data;
  },
  bulkAction: async (payload: {
    action: 'ban' | 'unban' | 'reset_traffic' | 'delete' | 'assign_plan' | 'assign_nodes';
    clientIds: string[];
    planId?: string;
    nodeIds?: string[];
    nodeMode?: 'replace' | 'add' | 'remove' | 'all';
    allNodes?: boolean;
  }): Promise<{ affected: number }> => {
    const { data } = await api.post<{ affected: number }>('/clients/bulk', payload);
    return data;
  },
  migrate: async (payload: {
    source: 'marzban' | 'hiddify';
    data: unknown;
    dryRun?: boolean;
  }): Promise<{ created: number; updated?: number; skipped: number; errors?: string[] }> => {
    const { data } = await api.post<{
      created: number;
      updated?: number;
      skipped: number;
      errors?: string[];
    }>('/clients/migrate', payload);
    return data;
  },
  regenerateSubToken: async (subToken: string): Promise<{ subToken: string }> => {
    const { data } = await api.post<{ subToken: string }>(`/client/${encodeURIComponent(subToken)}/regenerate`);
    return data;
  },
  getTrafficDaily: async (id: string, days = 30): Promise<TrafficChartPoint[]> => {
    const { data } = await api.get<TrafficChartPoint[]>(`/clients/${id}/traffic-daily`, { params: { days } });
    return Array.isArray(data) ? data : [];
  },
  getStats: async (id: string, days = 30): Promise<Array<{ nodeId: string; nodeName?: string; upload: number; download: number }>> => {
    const { data } = await api.get<
      Array<{ nodeId: string; nodeName?: string; upload: number; download: number }>
    >(`/clients/${id}/stats`, { params: { days } });
    return Array.isArray(data) ? data : [];
  },
};

export const nodesApi = {
  createJoinToken: async (
    name?: string,
  ): Promise<{ token: string; expiresAt: string; ttlMinutes: number; panelUrl: string; command: string }> => {
    const { data } = await api.post('/nodes/join-tokens', name ? { name } : {});
    return data;
  },
  list: async (params?: { search?: string; status?: string }): Promise<NodeItem[]> => {
    const { data } = await api.get<NodeItem[]>('/nodes', { params });
    return Array.isArray(data) ? data : [];
  },
  get: async (id: string): Promise<NodeItem> => {
    const { data } = await api.get<NodeItem>(`/nodes/${id}`);
    return data;
  },
  create: async (payload: Record<string, unknown>): Promise<NodeItem & {
    reachable?: boolean;
    isDomain?: boolean;
    panelUrl?: string;
    wsUrl?: string;
    scriptUrl?: string;
    binaryUrl?: string;
    command?: string;
    installCommand?: string;
    setupInstructions?: string;
  }> => {
    const { data } = await api.post<NodeItem & {
      reachable?: boolean;
      isDomain?: boolean;
      panelUrl?: string;
      wsUrl?: string;
      scriptUrl?: string;
      binaryUrl?: string;
      command?: string;
      installCommand?: string;
      setupInstructions?: string;
    }>('/nodes', payload);
    return data;
  },
  update: async (id: string, payload: Record<string, unknown>): Promise<NodeItem> => {
    const { data } = await api.put<NodeItem>(`/nodes/${id}`, payload);
    return data;
  },
  delete: async (id: string): Promise<void> => {
    await api.delete(`/nodes/${id}`);
  },
  check: async (id: string): Promise<Record<string, unknown>> => {
    const { data } = await api.post<Record<string, unknown>>(`/nodes/${id}/check`);
    return data;
  },
  pushConfig: async (id: string): Promise<Record<string, unknown>> => {
    const { data } = await api.post<Record<string, unknown>>(`/nodes/${id}/push-config`);
    return data;
  },
  restart: async (id: string): Promise<Record<string, unknown>> => {
    const { data } = await api.post<Record<string, unknown>>(`/nodes/${id}/restart`);
    return data;
  },
  stop: async (id: string): Promise<Record<string, unknown>> => {
    const { data } = await api.post<Record<string, unknown>>(`/nodes/${id}/stop`);
    return data;
  },
  getInbounds: async (id: string): Promise<Inbound[]> => {
    const { data } = await api.get<Inbound[]>(`/nodes/${id}/inbounds`);
    return Array.isArray(data) ? data : [];
  },
  getInstallCommand: async (id: string): Promise<{
    nodeId?: string;
    name?: string;
    host?: string;
    port?: number;
    apiPort?: number;
    secret?: string;
    isDomain?: boolean;
    panelUrl?: string;
    wsUrl?: string;
    scriptUrl?: string;
    binaryUrl?: string;
    command?: string;
    installCommand?: string;
    setupInstructions?: string;
  }> => {
    const { data } = await api.get<{
      nodeId?: string;
      name?: string;
      host?: string;
      port?: number;
      apiPort?: number;
      secret?: string;
      isDomain?: boolean;
      panelUrl?: string;
      wsUrl?: string;
      scriptUrl?: string;
      binaryUrl?: string;
      command?: string;
      installCommand?: string;
      setupInstructions?: string;
    }>(`/nodes/${id}/install-command`);
    return data;
  },
  getMtlsCert: async (id: string): Promise<{
    issued?: boolean;
    enabled?: boolean;
    fingerprint?: string | null;
    subject?: string | null;
    expiresAt?: string | null;
    caFingerprint?: string | null;
  }> => {
    const { data } = await api.get(`/nodes/${id}/mtls-cert`);
    return data;
  },
  rotateMtlsCert: async (id: string): Promise<{
    fingerprint?: string;
    expiresAt?: string;
    certPem?: string;
    caPem?: string;
  }> => {
    const { data } = await api.post(`/nodes/${id}/mtls-cert/rotate`);
    return data;
  },
  revokeMtlsCert: async (id: string): Promise<void> => {
    await api.delete(`/nodes/${id}/mtls-cert`);
  },
  coreUpdate: async (
    id: string,
    payload: { core: 'xray' | 'singbox' | 'naive' | 'mieru'; version?: string },
  ): Promise<{
    ok?: boolean;
    success?: boolean;
    core?: string;
    version?: string;
    status?: string;
    sha256?: string;
    message?: string;
  }> => {
    const { data } = await api.post(`/nodes/${id}/core-update`, payload, { timeout: 180_000 });
    return data;
  },
  getLogs: async (
    id: string,
    params?: { core?: string; limit?: number; stream?: string },
  ): Promise<NodeLogEntry[]> => {
    const { data } = await api.get<NodeLogEntry[] | { logs?: NodeLogEntry[] }>('/logs', {
      params: { nodeId: id, ...params },
    });
    return Array.isArray(data) ? data : data.logs ?? [];
  },
  getSessions: async (id: string): Promise<SessionEntry[]> => {
    const { data } = await api.get<SessionEntry[] | { sessions?: SessionEntry[] }>('/sessions', {
      params: { nodeId: id },
    });
    return Array.isArray(data) ? data : data.sessions ?? [];
  },
  dropSession: async (
    id: string,
    payload: { sessionId?: string; clientId?: string; ip?: string },
  ): Promise<void> => {
    await api.post('/sessions/drop', {
      nodeId: id,
      sessionId: payload.sessionId,
      clientId: payload.clientId,
      sourceIp: payload.ip,
      ip: payload.ip,
    });
  },
};

export const inboundsApi = {
  list: async (params?: { protocol?: string; nodeId?: string; search?: string }): Promise<Inbound[]> => {
    const { data } = await api.get<Inbound[]>('/inbounds', { params });
    return Array.isArray(data) ? data : [];
  },
  create: async (payload: Record<string, unknown>): Promise<Inbound> => {
    const { data } = await api.post<Inbound>('/inbounds', payload);
    return data;
  },
  update: async (id: string, payload: Record<string, unknown>): Promise<Inbound> => {
    const { data } = await api.put<Inbound>(`/inbounds/${id}`, payload);
    return data;
  },
  delete: async (id: string): Promise<void> => {
    await api.delete(`/inbounds/${id}`);
  },
  toggle: async (id: string): Promise<Inbound> => {
    const { data } = await api.post<Inbound>(`/inbounds/${id}/toggle`);
    return data;
  },
  createPortShare: async (inboundId: string, payload: Partial<PortShare>): Promise<PortShare> => {
    const { data } = await api.post<PortShare>(`/inbounds/${inboundId}/port-share`, payload);
    return data;
  },
  updatePortShare: async (
    inboundId: string,
    shareId: string,
    payload: Partial<PortShare>,
  ): Promise<PortShare> => {
    const { data } = await api.put<PortShare>(`/inbounds/${inboundId}/port-share/${shareId}`, payload);
    return data;
  },
  deletePortShare: async (inboundId: string, shareId: string): Promise<void> => {
    await api.delete(`/inbounds/${inboundId}/port-share/${shareId}`);
  },
  validate: async (config: unknown): Promise<import('@/types').XrayValidationResult> => {
    const { data } = await api.post<import('@/types').XrayValidationResult>('/inbounds/validate', { config });
    return data;
  },
  generateKeys: async (payload: {
    kind: 'reality' | 'mldsa65' | 'vlessenc' | 'ss2022' | 'uuid' | 'shortid';
    method?: string;
  }): Promise<Record<string, unknown>> => {
    const { data } = await api.post<Record<string, unknown>>('/inbounds/generate-keys', payload);
    return data;
  },
  getXrayProfile: async (nodeId: string): Promise<import('@/types').XrayNodeProfile> => {
    const { data } = await api.get<import('@/types').XrayNodeProfile>(`/inbounds/xray-profile/${nodeId}`);
    return data;
  },
  saveXrayProfile: async (
    nodeId: string,
    payload: { profileName?: string; config: unknown; push?: boolean; syncInbounds?: boolean },
  ): Promise<{ saved: boolean; pushed: boolean; validation: import('@/types').XrayValidationResult }> => {
    const { data } = await api.put(`/inbounds/xray-profile/${nodeId}`, payload);
    return data;
  },
};

export const hostsApi = {
  list: async (params?: { inboundId?: string; nodeId?: string; search?: string }): Promise<HostOverride[]> => {
    const { data } = await api.get<HostOverride[]>('/hosts', { params });
    return Array.isArray(data) ? data : [];
  },
  create: async (payload: Partial<HostOverride>): Promise<HostOverride> => {
    const { data } = await api.post<HostOverride>('/hosts', payload);
    return data;
  },
  update: async (id: string, payload: Partial<HostOverride>): Promise<HostOverride> => {
    const { data } = await api.put<HostOverride>(`/hosts/${id}`, payload);
    return data;
  },
  delete: async (id: string): Promise<void> => {
    await api.delete(`/hosts/${id}`);
  },
  toggle: async (id: string): Promise<HostOverride> => {
    const { data } = await api.post<HostOverride>(`/hosts/${id}/toggle`);
    return data;
  },
  bulk: async (payload: {
    action: 'enable' | 'disable' | 'delete';
    hostIds: string[];
  }): Promise<{ affected: number }> => {
    const { data } = await api.post<{ affected: number }>('/hosts/bulk', payload);
    return data;
  },
};

export const routingApi = {
  list: async (): Promise<RoutingRule[]> => {
    const { data } = await api.get<RoutingRule[]>('/routing');
    return Array.isArray(data) ? data : [];
  },
  create: async (payload: Partial<RoutingRule>): Promise<RoutingRule> => {
    const { data } = await api.post<RoutingRule>('/routing', payload);
    return data;
  },
  update: async (id: string, payload: Partial<RoutingRule>): Promise<RoutingRule> => {
    const { data } = await api.put<RoutingRule>(`/routing/${id}`, payload);
    return data;
  },
  delete: async (id: string): Promise<void> => {
    await api.delete(`/routing/${id}`);
  },
  toggle: async (id: string): Promise<RoutingRule> => {
    const { data } = await api.post<RoutingRule>(`/routing/${id}/toggle`);
    return data;
  },
  reorder: async (rules: Array<{ id: string; priority: number }>): Promise<void> => {
    await api.post('/routing/reorder', {
      ruleIds: rules.map((r) => r.id),
      rules,
    });
  },
  presets: async (): Promise<Array<{ id: string; name: string; description?: string; rule: Partial<RoutingRule> }>> => {
    const { data } = await api.get<Array<Record<string, unknown>>>('/routing/presets/list');
    if (!Array.isArray(data)) return [];
    return data.map((item, idx) => ({
      id: String(item.id ?? `preset-${idx}`),
      name: String(item.name ?? 'Preset'),
      description: typeof item.description === 'string' ? item.description : undefined,
      rule:
        item.rule && typeof item.rule === 'object'
          ? (item.rule as Partial<RoutingRule>)
          : ({
              name: typeof item.name === 'string' ? item.name : undefined,
              domain: Array.isArray(item.domain) ? (item.domain as string[]) : undefined,
              ip: Array.isArray(item.ip) ? (item.ip as string[]) : undefined,
              port: typeof item.port === 'string' ? item.port : undefined,
              protocol: Array.isArray(item.protocol) ? (item.protocol as string[]) : undefined,
              outboundTag: typeof item.outboundTag === 'string' ? item.outboundTag : 'block',
              priority: typeof item.priority === 'number' ? item.priority : undefined,
            } as Partial<RoutingRule>),
    }));
  },
};

export const plansApi = {
  list: async (): Promise<Plan[]> => {
    const { data } = await api.get<Plan[]>('/plans');
    return Array.isArray(data) ? data : [];
  },
  create: async (payload: Partial<Plan>): Promise<Plan> => {
    const { data } = await api.post<Plan>('/plans', payload);
    return data;
  },
  update: async (id: string, payload: Partial<Plan>): Promise<Plan> => {
    const { data } = await api.put<Plan>(`/plans/${id}`, payload);
    return data;
  },
  delete: async (id: string): Promise<void> => {
    await api.delete(`/plans/${id}`);
  },
};

export const resellersApi = {
  list: async (): Promise<Reseller[]> => {
    const { data } = await api.get<Reseller[]>('/resellers');
    return Array.isArray(data) ? data : [];
  },
  create: async (payload: Partial<Reseller>): Promise<Reseller> => {
    const { data } = await api.post<Reseller>('/resellers', payload);
    return data;
  },
  update: async (id: string, payload: Partial<Reseller>): Promise<Reseller> => {
    const { data } = await api.put<Reseller>(`/resellers/${id}`, payload);
    return data;
  },
  delete: async (id: string): Promise<void> => {
    await api.delete(`/resellers/${id}`);
  },
  stats: async (id: string): Promise<{
    clients: { total: number; active: number };
    usedTraffic: number;
    trafficLimit: number;
    todayTraffic?: number;
  }> => {
    const { data } = await api.get<Record<string, unknown>>(`/resellers/${id}/stats`);
    const limits = (data?.limits ?? {}) as Record<string, unknown>;
    const traffic = (data?.traffic ?? {}) as Record<string, unknown>;
    const today = (traffic?.today ?? {}) as Record<string, unknown>;
    const clients = (data?.clients ?? {}) as Record<string, unknown>;
    return {
      clients: {
        total: Number(clients.total ?? 0),
        active: Number(clients.active ?? 0),
      },
      usedTraffic: Number(data?.usedTraffic ?? limits.usedTraffic ?? 0),
      trafficLimit: Number(data?.trafficLimit ?? limits.trafficLimit ?? 0),
      todayTraffic: Number(
        data?.todayTraffic ?? (Number(today.upload ?? 0) + Number(today.download ?? 0)),
      ),
    };
  },
};

export const webhooksApi = {
  list: async (): Promise<WebhookItem[]> => {
    const { data } = await api.get<WebhookItem[]>('/webhooks');
    return Array.isArray(data) ? data : [];
  },
  create: async (payload: Partial<WebhookItem>): Promise<WebhookItem> => {
    const { data } = await api.post<WebhookItem>('/webhooks', payload);
    return data;
  },
  update: async (id: string, payload: Partial<WebhookItem>): Promise<WebhookItem> => {
    const { data } = await api.put<WebhookItem>(`/webhooks/${id}`, payload);
    return data;
  },
  delete: async (id: string): Promise<void> => {
    await api.delete(`/webhooks/${id}`);
  },
  test: async (id: string): Promise<{ status?: number; success?: boolean; error?: string }> => {
    const { data } = await api.post(`/webhooks/${id}/test`);
    return data;
  },
};

export const templatesApi = {
  list: async (): Promise<SubscriptionTemplate[]> => {
    const { data } = await api.get<SubscriptionTemplate[]>('/templates');
    return Array.isArray(data) ? data : [];
  },
  create: async (payload: Partial<SubscriptionTemplate>): Promise<SubscriptionTemplate> => {
    const { data } = await api.post<SubscriptionTemplate>('/templates', payload);
    return data;
  },
  update: async (id: string, payload: Partial<SubscriptionTemplate>): Promise<SubscriptionTemplate> => {
    const { data } = await api.put<SubscriptionTemplate>(`/templates/${id}`, payload);
    return data;
  },
  delete: async (id: string): Promise<void> => {
    await api.delete(`/templates/${id}`);
  },
  preview: async (id: string, clientId?: string): Promise<{ rendered: string }> => {
    const { data } = await api.post<{ rendered: string }>(`/templates/${id}/preview`, { clientId });
    return data;
  },
};

export const srrApi = {
  list: async (): Promise<SrrRule[]> => {
    const { data } = await api.get<SrrRule[]>('/templates/srr');
    return Array.isArray(data) ? data : [];
  },
  create: async (payload: Partial<SrrRule>): Promise<SrrRule> => {
    const { data } = await api.post<SrrRule>('/templates/srr', payload);
    return data;
  },
  update: async (id: string, payload: Partial<SrrRule>): Promise<SrrRule> => {
    const { data } = await api.put<SrrRule>(`/templates/srr/${id}`, payload);
    return data;
  },
  delete: async (id: string): Promise<void> => {
    await api.delete(`/templates/srr/${id}`);
  },
  reorder: async (ruleIds: string[]): Promise<void> => {
    await api.post('/templates/srr/reorder', { ruleIds });
  },
};

export const sessionsApi = {
  list: async (params?: { nodeId?: string; clientId?: string; search?: string }): Promise<SessionEntry[]> => {
    const { data } = await api.get<SessionEntry[] | { sessions?: SessionEntry[] }>('/sessions', { params });
    return Array.isArray(data) ? data : data.sessions ?? [];
  },
  drop: async (payload: {
    sessionId?: string;
    nodeId: string;
    clientId?: string;
    ip?: string;
  }): Promise<void> => {
    await api.post('/sessions/drop', { ...payload, sourceIp: payload.ip });
  },
};

export const inspectorsApi = {
  listSrh: async (params?: {
    clientId?: string;
    search?: string;
    page?: number;
    limit?: number;
  }): Promise<{ data: SrhEntry[]; total: number }> => {
    const { data } = await api.get<{ data?: SrhEntry[]; entries?: SrhEntry[]; total?: number } | SrhEntry[]>(
      '/inspector/srh',
      { params },
    );
    if (Array.isArray(data)) return { data, total: data.length };
    const items = data.data ?? data.entries ?? [];
    return { data: items, total: data.total ?? items.length };
  },
  listHwid: async (params?: { search?: string; page?: number; limit?: number }): Promise<{
    data: Array<DeviceItem & { username?: string; deviceLimit?: number | null }>;
    total: number;
  }> => {
    const { data } = await api.get('/inspector/hwid', { params });
    if (Array.isArray(data)) return { data, total: data.length };
    const items = data.data ?? data.devices ?? [];
    return { data: items, total: data.total ?? items.length };
  },
  listTorrentBlocker: async (params?: {
    nodeId?: string;
    page?: number;
    limit?: number;
  }): Promise<{ data: TorrentBlockerReport[]; total: number }> => {
    const { data } = await api.get('/inspector/torrent-reports', { params });
    if (Array.isArray(data)) return { data, total: data.length };
    const items = data.data ?? data.reports ?? [];
    return { data: items, total: data.total ?? items.length };
  },
};

export const infraBillingApi = {
  listProviders: async (): Promise<InfraProvider[]> => {
    const { data } = await api.get<InfraProvider[]>('/infra-billing/providers');
    return Array.isArray(data) ? data : [];
  },
  createProvider: async (payload: Partial<InfraProvider>): Promise<InfraProvider> => {
    const { data } = await api.post<InfraProvider>('/infra-billing/providers', payload);
    return data;
  },
  updateProvider: async (id: string, payload: Partial<InfraProvider>): Promise<InfraProvider> => {
    const { data } = await api.put<InfraProvider>(`/infra-billing/providers/${id}`, payload);
    return data;
  },
  deleteProvider: async (id: string): Promise<void> => {
    await api.delete(`/infra-billing/providers/${id}`);
  },
  listNodeBillings: async (): Promise<InfraNodeBilling[]> => {
    const { data } = await api.get<InfraNodeBilling[]>('/infra-billing/nodes');
    return Array.isArray(data) ? data : [];
  },
  createNodeBilling: async (payload: Partial<InfraNodeBilling>): Promise<InfraNodeBilling> => {
    const { data } = await api.post<InfraNodeBilling>('/infra-billing/nodes', payload);
    return data;
  },
  updateNodeBilling: async (id: string, payload: Partial<InfraNodeBilling>): Promise<InfraNodeBilling> => {
    const { data } = await api.put<InfraNodeBilling>(`/infra-billing/nodes/${id}`, payload);
    return data;
  },
  deleteNodeBilling: async (id: string): Promise<void> => {
    await api.delete(`/infra-billing/nodes/${id}`);
  },
};

export const dashboardApi = {
  overview: async (): Promise<DashboardOverview> => {
    const { data } = await api.get<DashboardOverview>('/dashboard/overview');
    return data;
  },
  trafficChart: async (days = 14): Promise<TrafficChartPoint[]> => {
    const { data } = await api.get<TrafficChartPoint[]>('/dashboard/traffic-chart', { params: { days } });
    return Array.isArray(data) ? data : [];
  },
  topClients: async (days = 7, limit = 6): Promise<TopClient[]> => {
    const { data } = await api.get<TopClient[]>('/dashboard/top-clients', { params: { days, limit } });
    return Array.isArray(data) ? data : [];
  },
  recentAudit: async (limit = 6): Promise<AuditLog[]> => {
    const { data } = await api.get<AuditLog[]>('/dashboard/recent-audit', { params: { limit } });
    return Array.isArray(data) ? data : [];
  },
};

export const monitoringApi = {
  host: async (): Promise<HostTelemetry> => {
    const { data } = await api.get<HostTelemetry>('/monitoring/host');
    return data;
  },
  node: async (id: string): Promise<{
    nodeId: string;
    metrics: NodeMonitoringMetrics;
  }> => {
    const { data } = await api.get(`/monitoring/nodes/${id}`);
    return data;
  },
};

export const auditApi = {
  list: async (params?: {
    action?: string;
    resource?: string;
    startDate?: string;
    endDate?: string;
    page?: number;
    limit?: number;
  }): Promise<{ logs: AuditLog[]; total: number; page?: number; pages?: number }> => {
    const { data } = await api.get('/audit', { params: { limit: 30, ...params } });
    return data;
  },
  resources: async (): Promise<string[]> => {
    const { data } = await api.get<Array<string | { resource?: string }>>('/audit/resources');
    if (!Array.isArray(data)) return [];
    return data
      .map((item) => (typeof item === 'string' ? item : item?.resource ?? ''))
      .filter(Boolean);
  },
  actions: async (): Promise<string[]> => {
    const { data } = await api.get<Array<string | { action?: string }>>('/audit/actions');
    if (!Array.isArray(data)) return [];
    return data
      .map((item) => (typeof item === 'string' ? item : item?.action ?? ''))
      .filter(Boolean);
  },
};

export const settingsApi = {
  get: async (): Promise<Record<string, string>> => {
    const { data } = await api.get<Record<string, unknown>>('/settings');
    const out: Record<string, string> = {};
    if (data && typeof data === 'object') {
      for (const [k, v] of Object.entries(data)) {
        out[k] = v == null ? '' : typeof v === 'string' ? v : JSON.stringify(v);
      }
    }
    return out;
  },
  update: async (settings: Record<string, string>): Promise<void> => {
    await api.put('/settings', { settings });
  },
};

export const backupApi = {
  logs: async (): Promise<BackupLog[]> => {
    const { data } = await api.get<BackupLog[] | { data?: BackupLog[]; logs?: BackupLog[] }>('/backup/logs');
    const raw = Array.isArray(data) ? data : data.data ?? data.logs ?? [];
    return raw.map((item) => ({
      ...item,
      filename: item.filename || item.filePath?.split('/').pop(),
      sizeBytes: item.sizeBytes ?? item.fileSize,
    }));
  },
  config: async (): Promise<Record<string, unknown>> => {
    const { data } = await api.get('/backup/config');
    return data ?? {};
  },
  trigger: async (payload: { type: string; destination: string }): Promise<{ id: string; status: string }> => {
    const { data } = await api.post<{ id: string; status: string }>('/backup/trigger', payload);
    return data;
  },
  downloadUrl: (id: string): string => `/api/v1/backup/download/${encodeURIComponent(id)}`,
};

export const configTransferApi = {
  exportConfig: async (opts: {
    version?: 'v1' | 'v2';
    format?: 'json' | 'yaml';
    includeSecrets?: boolean;
  }): Promise<string> => {
    const path = opts.version === 'v2' ? '/config-transfer/v2/export' : '/config-transfer/export';
    const { data } = await api.get<string>(path, {
      params: { format: opts.format ?? 'json', includeSecrets: opts.includeSecrets ?? false },
      responseType: 'text',
      transformResponse: [(v) => v],
    });
    return typeof data === 'string' ? data : JSON.stringify(data, null, 2);
  },
  importConfig: async (opts: {
    version?: 'v1' | 'v2';
    mode?: 'create-only' | 'upsert';
    apply: boolean;
    content: string;
    format?: 'json' | 'yaml';
  }): Promise<{ created?: number; updated?: number; skipped?: number; warnings?: string[]; errors?: string[] }> => {
    const path = opts.version === 'v2' ? '/config-transfer/v2/import' : '/config-transfer/import';
    const { data } = await api.post(path, {
      mode: opts.mode ?? 'upsert',
      apply: opts.apply,
      dryRun: !opts.apply,
      format: opts.format ?? 'json',
      content: opts.content,
    });
    return data;
  },
  migrate: async (payload: {
    source: 'marzban' | 'hiddify';
    content: string;
    dryRun: boolean;
  }): Promise<{ created?: number; updated?: number; skipped?: number; errors?: string[] }> => {
    const { data } = await api.post('/config-transfer/migrate', payload);
    return data;
  },
};

export const coresApi = {
  versions: async (refresh = false): Promise<CoreVersionEntry[]> => {
    const { data } = await api.get<CoreVersionEntry[] | { cores?: CoreVersionEntry[] }>(
      '/cores/versions',
      refresh ? { params: { refresh: 'true' } } : undefined,
    );
    return Array.isArray(data) ? data : data.cores ?? [];
  },
};
