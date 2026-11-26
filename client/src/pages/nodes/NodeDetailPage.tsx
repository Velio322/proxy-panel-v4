import { useEffect, useMemo, useState } from 'react';
import { Link, useParams, useSearchParams } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import {
  Activity,
  ArrowLeft,
  Check,
  Clock,
  Copy,
  Cpu,
  Download,
  ExternalLink,
  Globe,
  HardDrive,
  Network,
  Plus,
  RefreshCw,
  Search,
  Server,
  Shield,
  Signal,
  Square,
  Terminal,
  Trash2,
  UploadCloud,
  Users,
  X,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { NodeInstallModal } from '@/components/nodes/NodeInstallModal';
import { NodeLogViewer } from '@/components/nodes/NodeLogViewer';
import {
  formatRelativeAgo,
  formatUptime,
  latencyBadgeClass,
} from '@/components/nodes/NodeTelemetryPrimitives';
import { XrayConfigStudioPanel } from '@/components/inbounds/XrayConfigStudioModal';
import { clientsApi, coresApi, getApiErrorMessage, monitoringApi, nodesApi } from '@/lib/api';
import { PROTOCOL_COLORS, cn, formatBps, formatBytes, formatDateTime } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { Protocol } from '@/types';
import { useCopyFeedback } from '@/hooks/use-copy-feedback';

const VALID_TABS = new Set([
  'telemetry',
  'clients',
  'xray-config',
  'mtls',
  'updates',
  'logs',
  'sessions',
]);

export function NodeDetailPage() {
  const { id = '' } = useParams<{ id: string }>();
  const [searchParams, setSearchParams] = useSearchParams();
  const { t } = useTranslation();
  const qc = useQueryClient();
  const timezone = useUiStore((s) => s.timezone);
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const rawTab = searchParams.get('tab') || 'telemetry';
  const activeTab = VALID_TABS.has(rawTab) ? rawTab : 'telemetry';
  const setActiveTab = (tab: string) => {
    setSearchParams({ tab }, { replace: true });
  };

  const [activeControl, setActiveControl] = useState<'check' | 'push' | 'restart' | 'stop' | null>(null);
  const [copiedHost, copyHost] = useCopyFeedback(1400);
  const [copiedFp, copyFp] = useCopyFeedback(1400);
  const [installModalOpen, setInstallModalOpen] = useState(false);

  const [targetVersions, setTargetVersions] = useState<Record<string, string>>({});
  const [updatingCore, setUpdatingCore] = useState<string | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [pinnedOnly, setPinnedOnly] = useState(false);
  const [clientSearch, setClientSearch] = useState('');
  const [liveSamples, setLiveSamples] = useState<Array<{ time: string; cpu: number; mem: number }>>([]);

  useEffect(() => {
    setLiveSamples([]);
  }, [id]);

  const { data: node } = useQuery({
    queryKey: ['node', id],
    queryFn: () => nodesApi.get(id),
    enabled: Boolean(id),
    refetchInterval: 10_000,
  });

  const { data: allNodes = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
    enabled: Boolean(id),
    refetchInterval: 15_000,
  });

  const { data: telemetry } = useQuery({
    queryKey: ['node-telemetry', id],
    queryFn: () => monitoringApi.node(id),
    enabled: Boolean(id),
    refetchInterval: 5_000,
  });

  const { data: inbounds = [] } = useQuery({
    queryKey: ['node-inbounds', id],
    queryFn: () => nodesApi.getInbounds(id),
    enabled: Boolean(id),
  });

  const { data: mtlsCert } = useQuery({
    queryKey: ['node-mtls', id],
    queryFn: () => nodesApi.getMtlsCert(id),
    enabled: Boolean(id),
  });

  const { data: sessions = [] } = useQuery({
    queryKey: ['node-sessions', id],
    queryFn: () => nodesApi.getSessions(id),
    enabled: Boolean(id),
    refetchInterval: 10_000,
  });

  const { data: coreCatalog = [] } = useQuery({
    queryKey: ['cores-versions'],
    queryFn: () => coresApi.versions(),
  });

  const { data: nodeClientsRes, isLoading: clientsLoading } = useQuery({
    queryKey: ['node-clients', id, pinnedOnly],
    queryFn: () => clientsApi.list({ nodeId: id, pinnedOnly: pinnedOnly || undefined, limit: 100 }),
    enabled: Boolean(id),
  });

  const metrics = telemetry?.metrics;
  const ht = metrics?.hostTelemetry;

  // Record live 5s samples to complement stored DB NodeMetric snapshots
  useEffect(() => {
    const cpu = metrics?.cpuUsage ?? ht?.cpu?.usagePercent;
    const mem = metrics?.memUsage ?? ht?.memory?.usagePercent;
    const stamp = ht?.sampledAt || metrics?.sampledAt;
    if (cpu == null || mem == null || !Number.isFinite(cpu) || !Number.isFinite(mem)) return;

    const label = new Date(stamp || Date.now()).toLocaleTimeString(isRu ? 'ru-RU' : 'en-US', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });

    setLiveSamples((prev) => {
      if (prev.length > 0 && prev[prev.length - 1].time === label) return prev;
      return [
        ...prev.slice(-23),
        {
          time: label,
          cpu: Number(cpu.toFixed(1)),
          mem: Number(mem.toFixed(1)),
        },
      ];
    });
  }, [metrics?.cpuUsage, metrics?.memUsage, ht?.cpu?.usagePercent, ht?.memory?.usagePercent, ht?.sampledAt, metrics?.sampledAt, isRu]);

  const fleetNode = useMemo(() => allNodes.find((n) => n.id === id), [allNodes, id]);

  const chartSeries = useMemo(() => {
    const historical = [...(fleetNode?.nodeMetrics ?? [])]
      .reverse()
      .filter((m) => m.cpuUsage != null || m.memUsage != null)
      .map((m) => ({
        time: new Date(m.recordedAt).toLocaleTimeString(isRu ? 'ru-RU' : 'en-US', {
          hour: '2-digit',
          minute: '2-digit',
        }),
        cpu: Number((m.cpuUsage ?? 0).toFixed(1)),
        mem: Number((m.memUsage ?? 0).toFixed(1)),
      }));

    if (liveSamples.length >= 2) {
      return [...historical.slice(-12), ...liveSamples].slice(-24);
    }
    return [...historical, ...liveSamples].slice(-24);
  }, [fleetNode?.nodeMetrics, liveSamples, isRu]);

  const filteredNodeClients = useMemo(() => {
    const list = nodeClientsRes?.data ?? [];
    const q = clientSearch.trim().toLowerCase();
    if (!q) return list;
    return list.filter(
      (c) =>
        c.username.toLowerCase().includes(q) ||
        (c.email ?? '').toLowerCase().includes(q) ||
        c.uuid.toLowerCase().includes(q),
    );
  }, [nodeClientsRes?.data, clientSearch]);

  const controlMut = useMutation({
    mutationFn: (action: 'check' | 'push' | 'restart' | 'stop') => {
      if (action === 'check') return nodesApi.check(id);
      if (action === 'push') return nodesApi.pushConfig(id);
      if (action === 'restart') return nodesApi.restart(id);
      return nodesApi.stop(id);
    },
    onMutate: (action) => setActiveControl(action),
    onSuccess: (_, action) => {
      qc.invalidateQueries({ queryKey: ['node', id] });
      qc.invalidateQueries({ queryKey: ['node-telemetry', id] });
      qc.invalidateQueries({ queryKey: ['nodes'] });
      const labels: Record<'check' | 'push' | 'restart' | 'stop', string> = isRu
        ? {
            check: 'Проверка ноды выполнена',
            push: 'Конфигурация применена',
            restart: 'Ядра перезапущены',
            stop: 'Ядра остановлены',
          }
        : {
            check: 'Node health check completed',
            push: 'Configuration synced to node',
            restart: 'Cores restarted',
            stop: 'Cores stopped',
          };
      toast({ title: labels[action], variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка операции' : 'Failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
    onSettled: () => setActiveControl(null),
  });

  const rotateMtlsMut = useMutation({
    mutationFn: () => nodesApi.rotateMtlsCert(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['node-mtls', id] });
      toast({
        title: isRu ? 'mTLS сертификат перевыпущен' : 'mTLS client certificate rotated',
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка выпуска mTLS' : 'Rotation failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const revokeMtlsMut = useMutation({
    mutationFn: () => nodesApi.revokeMtlsCert(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['node-mtls', id] });
      toast({
        title: isRu ? 'mTLS сертификат отозван' : 'mTLS certificate revoked',
        variant: 'success',
      });
    },
  });

  const coreUpdateMut = useMutation({
    mutationFn: (payload: { core: 'xray' | 'singbox' | 'naive' | 'mieru' | 'telemt'; version?: string }) =>
      nodesApi.coreUpdate(id, payload),
    onMutate: (vars) => setUpdatingCore(vars.core),
    onSuccess: (res, vars) => {
      qc.invalidateQueries({ queryKey: ['node', id] });
      qc.invalidateQueries({ queryKey: ['node-telemetry', id] });
      qc.invalidateQueries({ queryKey: ['nodes'] });
      toast({
        title: isRu
          ? `${vars.core}: обновлено${res.version ? ` до ${res.version}` : ''}`
          : `${vars.core} updated${res.version ? ` to ${res.version}` : ''}`,
        description: res.sha256 ? `SHA-256: ${res.sha256.slice(0, 16)}...` : res.message,
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка обновления ядра' : 'Update failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
    onSettled: () => setUpdatingCore(null),
  });

  const handleRefreshCatalog = async () => {
    setCheckingUpdates(true);
    try {
      const fresh = await coresApi.versions(true);
      qc.setQueryData(['cores-versions'], fresh);
      qc.invalidateQueries({ queryKey: ['node', id] });
      toast({
        title: isRu ? 'Список релизов обновлён' : 'Core catalog refreshed',
        variant: 'success',
      });
    } catch (err) {
      toast({
        title: isRu ? 'Не удалось проверить обновления' : 'Failed to check updates',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      });
    } finally {
      setCheckingUpdates(false);
    }
  };

  const dropSessionMut = useMutation({
    mutationFn: (payload: { sessionId?: string; clientId?: string; ip?: string }) =>
      nodesApi.dropSession(id, payload),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['node-sessions', id] });
      toast({ title: isRu ? 'Сессия завершена' : 'Session dropped', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Не удалось сбросить сессию' : 'Drop failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const unbindClientMut = useMutation({
    mutationFn: (clientId: string) =>
      clientsApi.bulkAction({
        action: 'assign_nodes',
        clientIds: [clientId],
        nodeMode: 'remove',
        nodeIds: [id],
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['node-clients', id] });
      qc.invalidateQueries({ queryKey: ['clients'] });
      toast({
        title: isRu ? 'Клиент отвязан от этой ноды' : 'Client unbound from this node',
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Не удалось отвязать клиента' : 'Unbind failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const handleOpenInstall = () => {
    setInstallModalOpen(true);
  };

  const handleCopyEndpoint = () => {
    if (!node) return;
    copyHost(`${node.host}:${node.port}`).catch(() => {});
  };

  const cpuPct = metrics?.cpuUsage ?? ht?.cpu?.usagePercent ?? node?.cpuUsage ?? null;
  const tcpCount = metrics?.tcpConnections ?? ht?.connections?.establishedTcp ?? null;
  const udpCount = metrics?.udpSockets ?? ht?.connections?.udpSockets ?? null;
  const uploadBps = metrics?.uploadBps ?? ht?.network?.uploadBps ?? null;
  const downloadBps = metrics?.downloadBps ?? ht?.network?.downloadBps ?? null;

  const cores: Array<{
    key: 'xray' | 'singbox' | 'naive' | 'mieru' | 'telemt';
    label: string;
    sourceLabel: string;
    installed?: string | null;
    running?: boolean;
    pid?: number | null;
  }> = [
    {
      key: 'xray',
      label: 'Xray-core',
      sourceLabel: 'gRPC Stats',
      installed: metrics?.cores?.xray?.version ?? node?.xrayVersion,
      running: metrics?.cores?.xray?.running ?? metrics?.xrayRunning,
      pid: metrics?.xrayPid,
    },
    {
      key: 'singbox',
      label: 'Sing-box',
      sourceLabel: 'Clash API',
      installed: metrics?.cores?.singbox?.version ?? node?.singboxVersion,
      running: metrics?.cores?.singbox?.running ?? metrics?.singboxRunning,
      pid: metrics?.singboxPid,
    },
    {
      key: 'naive',
      label: 'NaiveProxy (Caddy)',
      sourceLabel: 'Access Log',
      installed: metrics?.cores?.naive?.version ?? node?.naiveVersion,
      running: metrics?.cores?.naive?.running ?? metrics?.naiveRunning,
      pid: metrics?.naivePid,
    },
    {
      key: 'mieru',
      label: 'Mieru (mita)',
      sourceLabel: 'mita metrics',
      installed: metrics?.cores?.mieru?.version ?? node?.mieruVersion,
      running: metrics?.cores?.mieru?.running ?? metrics?.mieruRunning,
      pid: metrics?.mieruPid,
    },
    {
      key: 'telemt',
      label: 'telemt (Telegram WEB)',
      sourceLabel: 'Prometheus',
      installed: metrics?.cores?.telemt?.version ?? node?.telemtVersion,
      running: metrics?.cores?.telemt?.running,
      pid: null,
    },
  ];

  const locationParts = [node?.country, node?.city, node?.isp].filter(
    (s): s is string => Boolean(s && s.trim()),
  );

  return (
    <div className="space-y-5">
      {/* Command Hero Banner */}
      <div className="node-hero-grid surface-card rounded-xl border border-border/80 bg-card/95 p-2 sm:p-5 space-y-4">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="flex items-start gap-3">
            <Button variant="outline" size="icon" asChild className="mt-0.5 shrink-0">
              <Link to="/nodes" aria-label={isRu ? 'Назад к списку нод' : 'Back to nodes'}>
                <ArrowLeft className="h-4 w-4" />
              </Link>
            </Button>
            <div className="space-y-1.5">
              <div className="flex flex-wrap items-center gap-2">
                <span
                  className={cn(
                    'h-2.5 w-2.5 rounded-full',
                    node?.status === 'ONLINE'
                      ? 'bg-emerald-400 animate-pulse shadow-[0_0_8px_hsl(162_84%_43%/0.8)]'
                      : node?.status === 'MAINTENANCE'
                        ? 'bg-amber-400'
                        : node?.status === 'ERROR'
                          ? 'bg-red-500'
                          : 'bg-zinc-500',
                  )}
                />
                <h1 className="text-xl font-bold tracking-tight">{node?.name || id}</h1>
                {node && (
                  <Badge variant={node.status === 'ONLINE' ? 'success' : node.status === 'ERROR' ? 'destructive' : 'secondary'}>
                    {node.status}
                  </Badge>
                )}
                {node?.arch && (
                  <Badge variant="outline" className="font-mono text-[10px]">
                    {node.arch}
                  </Badge>
                )}
                {locationParts.length > 0 && (
                  <span className="inline-flex items-center gap-1 rounded-md border border-border/70 bg-background/60 px-2 py-0.5 text-xs text-muted-foreground">
                    <Globe className="h-3 w-3 text-primary" />
                    {locationParts.join(' · ')}
                  </span>
                )}
              </div>

              <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                {node && (
                  <button
                    type="button"
                    onClick={handleCopyEndpoint}
                    className="inline-flex items-center gap-1.5 rounded-md border border-border/70 bg-background/60 px-2 py-0.5 font-mono text-xs text-foreground transition-colors hover:border-primary/45 hover:text-primary"
                  >
                    <span>
                      {node.host}:{node.port}
                    </span>
                    {copiedHost ? (
                      <Check className="h-3 w-3 text-emerald-400" />
                    ) : (
                      <Copy className="h-3 w-3 text-muted-foreground" />
                    )}
                  </button>
                )}
                <span className="font-mono">API :{node?.apiPort ?? '—'}</span>
                {(node?.tags ?? []).map((tag) => (
                  <Badge key={tag} variant="outline" className="text-[10px]">
                    #{tag}
                  </Badge>
                ))}
              </div>
            </div>
          </div>

          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              loading={activeControl === 'check'}
              disabled={Boolean(activeControl)}
              onClick={() => controlMut.mutate('check')}
            >
              <Activity className="h-4 w-4 text-primary" /> {isRu ? 'Проверить' : 'Check'}
            </Button>
            <Button
              size="sm"
              variant="outline"
              loading={activeControl === 'push'}
              disabled={Boolean(activeControl)}
              onClick={() => controlMut.mutate('push')}
            >
              <UploadCloud className="h-4 w-4" /> {isRu ? 'Применить конфиг' : 'Push Config'}
            </Button>
            <Button
              size="sm"
              variant="outline"
              loading={activeControl === 'restart'}
              disabled={Boolean(activeControl)}
              onClick={() => controlMut.mutate('restart')}
            >
              <RefreshCw className="h-4 w-4" /> {isRu ? 'Перезапустить' : 'Restart'}
            </Button>
            <Button size="sm" variant="outline" onClick={handleOpenInstall}>
              <Terminal className="h-4 w-4" /> {isRu ? 'Установка' : 'Setup'}
            </Button>
            <Button
              size="sm"
              variant="destructive"
              loading={activeControl === 'stop'}
              disabled={Boolean(activeControl)}
              onClick={() => controlMut.mutate('stop')}
            >
              <Square className="h-4 w-4" /> {isRu ? 'Остановить' : 'Stop'}
            </Button>
          </div>
        </div>

        {/* Live Telemetry Ribbon */}
        <div className="grid gap-2 border-t border-border/60 pt-3 text-xs sm:grid-cols-2 lg:grid-cols-5">
          <div className="flex items-center justify-between rounded-lg border border-border/60 bg-background/50 px-3 py-2">
            <span className="text-muted-foreground flex items-center gap-1.5">
              <Signal className="h-3.5 w-3.5" /> {isRu ? 'Пинг' : 'Ping'}
            </span>
            <span
              className={cn(
                'rounded border px-1.5 py-0.5 font-mono text-[11px] font-semibold tabular-nums',
                latencyBadgeClass(node?.lastPingMs),
              )}
            >
              {node?.lastPingMs != null ? `${node.lastPingMs} ms` : '—'}
            </span>
          </div>

          <div className="flex items-center justify-between rounded-lg border border-border/60 bg-background/50 px-3 py-2">
            <span className="text-muted-foreground flex items-center gap-1.5">
              <Clock className="h-3.5 w-3.5" /> {isRu ? 'Аптайм хоста' : 'Host Uptime'}
            </span>
            <span className="font-mono font-semibold tabular-nums">
              {formatUptime(ht?.uptime?.hostSeconds, isRu)}
            </span>
          </div>

          <div className="flex items-center justify-between rounded-lg border border-border/60 bg-background/50 px-3 py-2">
            <span className="text-muted-foreground flex items-center gap-1.5">
              <Server className="h-3.5 w-3.5" /> {isRu ? 'Аптайм агента' : 'Agent Uptime'}
            </span>
            <span className="font-mono font-semibold tabular-nums">
              {formatUptime(metrics?.uptime ?? ht?.uptime?.processSeconds, isRu)}
            </span>
          </div>

          <div className="flex items-center justify-between rounded-lg border border-border/60 bg-background/50 px-3 py-2">
            <span className="text-muted-foreground flex items-center gap-1.5">
              <Users className="h-3.5 w-3.5" /> {isRu ? 'Активные ключи' : 'Online Users'}
            </span>
            <span className="font-mono font-semibold text-emerald-400 tabular-nums">
              {metrics?.onlineUsersCount ?? '—'}
            </span>
          </div>

          <div className="flex items-center justify-between rounded-lg border border-border/60 bg-background/50 px-3 py-2">
            <span className="text-muted-foreground flex items-center gap-1.5">
              <Shield className="h-3.5 w-3.5" /> mTLS
            </span>
            <span className="font-mono text-[11px]">
              {mtlsCert?.issued || mtlsCert?.enabled ? (
                <Badge variant="success" className="text-[10px]">
                  {isRu ? 'Активен' : 'Issued'}
                </Badge>
              ) : (
                <span className="text-muted-foreground">{formatRelativeAgo(node?.lastCheckAt, isRu)}</span>
              )}
            </span>
          </div>
        </div>
      </div>

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="flex-wrap">
          <TabsTrigger value="telemetry">
            {isRu ? `Обзор и протоколы (${inbounds.length})` : `Overview & Protocols (${inbounds.length})`}
          </TabsTrigger>
          <TabsTrigger value="clients">
            {isRu
              ? `Клиенты (${nodeClientsRes?.total ?? filteredNodeClients.length})`
              : `Clients (${nodeClientsRes?.total ?? filteredNodeClients.length})`}
          </TabsTrigger>
          <TabsTrigger value="xray-config">{isRu ? 'Конфиг Xray' : 'Xray Config'}</TabsTrigger>
          <TabsTrigger value="mtls">{isRu ? 'mTLS сертификат' : 'mTLS Certificates'}</TabsTrigger>
          <TabsTrigger value="updates">{isRu ? 'Обновление ядер' : 'Core Auto-Update'}</TabsTrigger>
          <TabsTrigger value="logs">{isRu ? 'Логи' : 'Live Logs'}</TabsTrigger>
          <TabsTrigger value="sessions">
            {isRu ? `Сессии (${sessions.length})` : `Active Sessions (${sessions.length})`}
          </TabsTrigger>
        </TabsList>

        {/* Overview & Telemetry Tab */}
        <TabsContent value="telemetry" className="space-y-4">
          {/* Row 1: host vitals chip strip */}
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded-xl border border-border/80 bg-background/60 px-3 py-2 text-[11px]">
            <span className="inline-flex items-center gap-1 font-mono tabular-nums">
              <Cpu className="h-3 w-3 text-emerald-400" />
              <span className="font-bold text-foreground">{cpuPct != null ? `${cpuPct.toFixed(0)}%` : '—'}</span>
              CPU
              <span className="text-muted-foreground">· {ht?.cpu?.count != null ? `${ht.cpu.count} vCPU` : ''}</span>
              <span>load {ht?.cpu?.loadAverage ? ht.cpu.loadAverage.map((v) => v.toFixed(2)).join('/') : '—'}</span>
            </span>
            <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
              <HardDrive className="h-3 w-3 text-cyan-400" />
              <span className="font-semibold text-foreground">
                {ht?.memory?.usedBytes != null && ht?.memory?.totalBytes != null
                  ? `${formatBytes(ht.memory.usedBytes, { compact: true, locale: language })} / ${formatBytes(ht.memory.totalBytes, { compact: true, locale: language })}`
                  : '—'}
              </span>
              <span>RSS {ht?.process?.memory?.rss != null ? formatBytes(ht.process.memory.rss, { compact: true, locale: language }) : '—'}</span>
            </span>
            <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
              <Network className="h-3 w-3 text-primary" />
              <span className="text-emerald-400 font-semibold">↓ {formatBps(downloadBps, language)}</span>
              <span className="text-cyan-400 font-semibold">↑ {formatBps(uploadBps, language)}</span>
            </span>
            <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
              <Users className="h-3 w-3 text-emerald-400" />
              <span className="font-bold text-foreground">{metrics?.onlineUsersCount ?? 0}</span>
              {isRu ? 'онлайн' : 'online'}
              <span>· TCP {tcpCount ?? '—'}</span>
              <span>· UDP {udpCount ?? '—'}</span>
            </span>
            <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
              <Network className="h-3 w-3 text-teal-400" />
              <span className="text-foreground">↓ {formatBytes(metrics?.traffic?.total?.download ?? node?.netDownload ?? 0, { compact: true, locale: language })}</span>
              <span>· TX {formatBytes(ht?.network?.transmittedBytes ?? 0, { compact: true, locale: language })}</span>
            </span>
          </div>

          {/* Row 2: 4-Core Engine Matrix (7/12) + Assigned Inbounds (5/12) */}
          <div className="grid gap-3 lg:grid-cols-12">
            <Card className="lg:col-span-7">
              <CardHeader className="flex flex-row items-center justify-between pb-2">
                <CardTitle>{isRu ? 'Движки прокси (Ядра ноды)' : 'Proxy Core Engines'}</CardTitle>
                <Button size="sm" variant="outline" onClick={() => setActiveTab('updates')}>
                  {isRu ? 'Управление версиями' : 'Manage Updates'}
                </Button>
              </CardHeader>
              <CardContent>
                <div className="grid gap-2 sm:grid-cols-2">
                  {cores.map((c) => {
                    const isRunning = c.running === true;
                    return (
                      <div
                        key={c.key}
                        className="rounded-lg border border-border/75 bg-background/55 p-3 space-y-2"
                      >
                        <div className="flex items-center justify-between gap-2">
                          <div className="flex items-center gap-2">
                            <span
                              className={cn(
                                'h-2 w-2 rounded-full',
                                isRunning ? 'bg-emerald-400 animate-pulse' : 'bg-zinc-500',
                              )}
                            />
                            <span className="font-semibold text-sm">{c.label}</span>
                          </div>
                          <Badge variant={isRunning ? 'success' : 'secondary'} className="text-[10px]">
                            {isRunning
                              ? isRu
                                ? 'Работает'
                                : 'Running'
                              : isRu
                                ? 'Остановлен'
                                : 'Stopped'}
                          </Badge>
                        </div>

                        <div className="flex items-center justify-between text-xs text-muted-foreground font-mono">
                          <span>v{c.installed || '—'}</span>
                          <span>{c.pid ? `PID ${c.pid}` : c.sourceLabel}</span>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </CardContent>
            </Card>

            <Card className="lg:col-span-5">
              <CardHeader className="flex flex-row items-center justify-between pb-2">
                <CardTitle>{isRu ? 'Протоколы и порты' : 'Assigned Inbounds'}</CardTitle>
                <Button size="sm" variant="outline" asChild>
                  <Link to="/inbounds?create=1">
                    <Plus className="h-3.5 w-3.5" /> {isRu ? 'Добавить' : 'Add'}
                  </Link>
                </Button>
              </CardHeader>
              <CardContent className="space-y-2">
                {inbounds.length === 0 ? (
                  <div className="py-6 text-center text-xs text-muted-foreground">
                    {isRu ? 'На этой ноде пока нет протоколов.' : 'No inbounds assigned to this node.'}
                  </div>
                ) : (
                  inbounds.map((ib) => (
                    <div
                      key={ib.id}
                      className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border/75 bg-background/55 px-3 py-2 text-xs"
                    >
                      <div className="flex items-center gap-2 min-w-0">
                        <span
                          className={cn(
                            'h-2 w-2 shrink-0 rounded-full',
                            ib.enable ? 'bg-emerald-400' : 'bg-zinc-500',
                          )}
                        />
                        <Badge
                          className={cn('border text-[10px]', PROTOCOL_COLORS[ib.protocol as Protocol])}
                        >
                          {ib.protocol}
                        </Badge>
                        <span className="font-medium break-words min-w-0">{ib.tag}</span>
                      </div>
                      <div className="flex items-center gap-1.5 font-mono text-xs">
                        {ib.portShares && ib.portShares.length > 0 && (
                          <Badge variant="outline" className="text-[10px]">
                            +{ib.portShares.length} share
                          </Badge>
                        )}
                        <span className="rounded bg-muted/60 px-2 py-0.5">
                          {ib.listen}:{ib.port}
                        </span>
                      </div>
                    </div>
                  ))
                )}
              </CardContent>
            </Card>
          </div>

          {/* Row 3: Live CPU & RAM Telemetry History Chart */}
          <Card>
            <CardHeader className="flex flex-row items-center justify-between pb-2">
              <CardTitle>{isRu ? 'График нагрузки (CPU и RAM)' : 'Live Resource Load History'}</CardTitle>
              <div className="flex items-center gap-3 text-xs font-mono">
                <span className="inline-flex items-center gap-1.5 text-emerald-400">
                  <span className="h-2 w-2 rounded-full bg-emerald-400" /> CPU %
                </span>
                <span className="inline-flex items-center gap-1.5 text-cyan-400">
                  <span className="h-2 w-2 rounded-full bg-cyan-400" /> RAM %
                </span>
              </div>
            </CardHeader>
            <CardContent className="h-56">
              {chartSeries.length === 0 ? (
                <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
                  {isRu ? 'Сбор точек телеметрии...' : 'Collecting live telemetry samples...'}
                </div>
              ) : (
                <ResponsiveContainer width="100%" height="100%">
                  <AreaChart data={chartSeries}>
                    <defs>
                      <linearGradient id="nodeCpuGrad" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="5%" stopColor="#10b981" stopOpacity={0.35} />
                        <stop offset="95%" stopColor="#10b981" stopOpacity={0.02} />
                      </linearGradient>
                      <linearGradient id="nodeMemGrad" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="5%" stopColor="#06b6d4" stopOpacity={0.32} />
                        <stop offset="95%" stopColor="#06b6d4" stopOpacity={0.02} />
                      </linearGradient>
                    </defs>
                    <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" />
                    <XAxis
                      dataKey="time"
                      tick={{ fontSize: 11, fill: 'hsl(var(--muted-foreground))' }}
                    />
                    <YAxis
                      domain={[0, 100]}
                      width={42}
                      tickFormatter={(v) => `${v}%`}
                      tick={{ fontSize: 11, fill: 'hsl(var(--muted-foreground))' }}
                    />
                    <Tooltip
                      formatter={(val: number, key: string) => [
                        `${val.toFixed(1)}%`,
                        key === 'cpu' ? 'CPU' : 'RAM',
                      ]}
                      contentStyle={{
                        backgroundColor: 'hsl(var(--popover))',
                        borderColor: 'hsl(var(--border))',
                        borderRadius: '10px',
                        color: 'hsl(var(--popover-foreground))',
                        fontSize: '12px',
                      }}
                    />
                    <Area
                      type="monotone"
                      dataKey="cpu"
                      stroke="#10b981"
                      strokeWidth={2}
                      fill="url(#nodeCpuGrad)"
                    />
                    <Area
                      type="monotone"
                      dataKey="mem"
                      stroke="#06b6d4"
                      strokeWidth={2}
                      fill="url(#nodeMemGrad)"
                    />
                  </AreaChart>
                </ResponsiveContainer>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        {/* mTLS Tab */}
        <TabsContent value="mtls">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Shield className="h-4 w-4 text-primary" />
                {isRu ? 'Сертификат mTLS (Панель ↔ Нода)' : 'Panel ↔ Node mTLS Certificate'}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4 text-xs">
              <div className="grid gap-2 sm:grid-cols-2 rounded-lg border bg-muted/20 p-3">
                <div>
                  <span className="text-muted-foreground">{isRu ? 'Статус:' : 'Status:'}</span>{' '}
                  <Badge variant={mtlsCert?.issued || mtlsCert?.enabled ? 'success' : 'secondary'}>
                    {mtlsCert?.issued || mtlsCert?.enabled
                      ? isRu
                        ? 'Выпущен'
                        : 'Enabled'
                      : isRu
                        ? 'Не выпущен'
                        : 'Not Issued'}
                  </Badge>
                </div>
                <div>
                  <span className="text-muted-foreground">{isRu ? 'Истекает:' : 'Expires:'}</span>{' '}
                  {mtlsCert?.expiresAt ? formatDateTime(mtlsCert.expiresAt, timezone) : '—'}
                </div>
                <div className="sm:col-span-2 flex items-center justify-between gap-2 font-mono break-all">
                  <div>
                    <span className="text-muted-foreground">
                      {isRu ? 'Отпечаток (Fingerprint):' : 'Fingerprint:'}
                    </span>{' '}
                    {mtlsCert?.fingerprint || '—'}
                  </div>
                  {mtlsCert?.fingerprint && (
                    <Button
                      size="sm"
                      variant="ghost"
                      className="h-7 px-2 shrink-0"
                      onClick={async () => {
                        copyFp(mtlsCert.fingerprint || '').catch(() => {});
                      }}
                    >
                      {copiedFp ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
                    </Button>
                  )}
                </div>
                <div className="sm:col-span-2 font-mono break-all">
                  <span className="text-muted-foreground">CA Fingerprint:</span>{' '}
                  {mtlsCert?.caFingerprint || '—'}
                </div>
              </div>
              <div className="flex gap-2">
                <Button size="sm" onClick={() => rotateMtlsMut.mutate()} loading={rotateMtlsMut.isPending}>
                  <RefreshCw className="h-3.5 w-3.5" />
                  {isRu ? 'Выпустить / Перевыпустить сертификат' : 'Issue / Rotate Cert'}
                </Button>
                {(mtlsCert?.issued || mtlsCert?.enabled) && (
                  <Button size="sm" variant="destructive" onClick={() => revokeMtlsMut.mutate()}>
                    {isRu ? 'Отозвать сертификат' : 'Revoke Cert'}
                  </Button>
                )}
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Core Updates Tab */}
        <TabsContent value="updates">
          <Card>
            <CardHeader className="flex flex-wrap flex-row items-center justify-between gap-3">
              <div>
                <CardTitle>
                  {isRu ? 'Обновление ядер (с проверкой SHA-256)' : 'SHA-256 Verified Core Auto-Updates'}
                </CardTitle>
                <p className="mt-1 text-xs text-muted-foreground">
                  {isRu
                    ? 'Скачивание официальных релизов с GitHub, проверка контрольной суммы SHA-256 и атомарная замена с откатом.'
                    : 'Downloads official GitHub releases, verifies SHA-256 checksums, and swaps binaries atomically with rollback.'}
                </p>
              </div>
              <Button size="sm" variant="outline" onClick={handleRefreshCatalog} loading={checkingUpdates}>
                <RefreshCw className="h-3.5 w-3.5" />
                {isRu ? 'Проверить обновления' : 'Check Updates'}
              </Button>
            </CardHeader>
            <CardContent className="space-y-3">
              {cores.map((c) => {
                const catalog = coreCatalog.find((entry) => entry.core === c.key);
                const installedNorm = c.installed?.trim().replace(/^v/i, '');
                const latestNorm = catalog?.latestVersion?.trim().replace(/^v/i, '');
                const isUpToDate = Boolean(
                  installedNorm &&
                    latestNorm &&
                    (installedNorm === latestNorm || latestNorm.startsWith(`${installedNorm}-`)),
                );
                const hasUpdate = Boolean(installedNorm && latestNorm && !isUpToDate);
                return (
                  <div
                    key={c.key}
                    className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3 text-xs"
                  >
                    <div className="space-y-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="font-semibold text-sm">{c.label}</span>
                        {isUpToDate && (
                          <Badge variant="success" className="text-[10px]">
                            {isRu ? 'Актуально' : 'Up to date'}
                          </Badge>
                        )}
                        {hasUpdate && (
                          <Badge variant="warning" className="text-[10px]">
                            {isRu ? 'Доступно обновление' : 'Update available'}
                          </Badge>
                        )}
                        {catalog?.releaseUrl && (
                          <a
                            href={catalog.releaseUrl}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="inline-flex items-center gap-1 text-[11px] text-primary hover:underline"
                          >
                            GitHub <ExternalLink className="h-3 w-3" />
                          </a>
                        )}
                      </div>
                      <div className="text-muted-foreground">
                        {isRu ? 'Установлено:' : 'Installed:'}{' '}
                        <span className="font-mono text-foreground">
                          {c.installed || (isRu ? 'не определено' : 'unknown')}
                        </span>
                        {catalog?.latestVersion && (
                          <>
                            {' · '}
                            {isRu ? 'Последний релиз:' : 'Latest:'}{' '}
                            <span className="font-mono text-foreground">{catalog.latestVersion}</span>
                          </>
                        )}
                      </div>
                      {catalog?.sha256 && (
                        <div className="font-mono text-[11px] text-muted-foreground break-all">
                          SHA-256: {catalog.sha256}
                        </div>
                      )}
                      {catalog?.error && (
                        <div className="text-[11px] text-amber-400">
                          {isRu ? 'Ошибка каталога:' : 'Catalog error:'} {catalog.error}
                        </div>
                      )}
                    </div>
                    <div className="flex items-center gap-2">
                      <Input
                        placeholder={catalog?.latestVersion || (isRu ? 'последняя' : 'latest')}
                        value={targetVersions[c.key] ?? ''}
                        onChange={(e) =>
                          setTargetVersions((prev) => ({ ...prev, [c.key]: e.target.value }))
                        }
                        className="h-8 w-36 font-mono text-xs"
                      />
                      <Button
                        size="sm"
                        onClick={() =>
                          coreUpdateMut.mutate({
                            core: c.key,
                            version: targetVersions[c.key]?.trim() || undefined,
                          })
                        }
                        loading={updatingCore === c.key}
                        disabled={Boolean(updatingCore) || node?.status === 'OFFLINE'}
                      >
                        <Download className="h-3.5 w-3.5" />
                        {isRu ? 'Обновить' : 'Update'}
                      </Button>
                    </div>
                  </div>
                );
              })}
            </CardContent>
          </Card>
        </TabsContent>

        {/* Live Logs Tab */}
        <TabsContent value="logs">
          <Card>
            <CardHeader>
              <CardTitle>{isRu ? 'Логи ноды в реальном времени' : 'Live Node Logs'}</CardTitle>
            </CardHeader>
            <CardContent>
              <NodeLogViewer nodeId={id} />
            </CardContent>
          </Card>
        </TabsContent>

        {/* Active Sessions Tab */}
        <TabsContent value="sessions">
          <Card>
            <CardHeader>
              <CardTitle>{isRu ? 'Активные сессии на ноде' : 'Active Node Sessions'}</CardTitle>
            </CardHeader>
            <CardContent>
              {sessions.length === 0 ? (
                <div className="py-6 text-center text-xs text-muted-foreground">
                  {isRu ? 'Активных сессий пока не зафиксировано.' : 'No active sessions reported.'}
                </div>
              ) : (
                <div className="overflow-x-auto">
                  <table className="w-full text-xs">
                    <thead>
                      <tr className="border-b text-left text-muted-foreground">
                        <th className="py-1.5 px-2">{isRu ? 'Клиент' : 'Client'}</th>
                        <th className="py-1.5 px-2">{isRu ? 'Протокол / Тег' : 'Protocol / Tag'}</th>
                        <th className="py-1.5 px-2">IP</th>
                        <th className="py-1.5 px-2">{isRu ? 'Трафик' : 'Traffic'}</th>
                        <th className="py-1.5 px-2">{isRu ? 'Подключён' : 'Connected'}</th>
                        <th className="py-1.5 px-2 text-right">{isRu ? 'Действие' : 'Action'}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {sessions.map((s) => (
                        <tr key={s.id} className="border-b last:border-b-0">
                          <td className="py-1.5 px-2 font-medium">{s.username || s.clientId || '—'}</td>
                          <td className="py-1.5 px-2">
                            {s.protocol ? (
                              <Badge
                                className={cn(
                                  'border text-[10px] mr-1',
                                  PROTOCOL_COLORS[s.protocol as Protocol] || '',
                                )}
                              >
                                {s.protocol}
                              </Badge>
                            ) : (
                              '— '
                            )}
                            {s.inboundTag ? `(${s.inboundTag})` : ''}
                          </td>
                          <td className="py-1.5 px-2 font-mono">{s.sourceIp}</td>
                          <td className="py-1.5 px-2 font-mono tabular-nums">
                            ↑ {formatBytes(s.uploadBytes, { locale: language })} / ↓{' '}
                            {formatBytes(s.downloadBytes, { locale: language })}
                          </td>
                          <td className="py-1.5 px-2">{formatDateTime(s.connectedAt, timezone)}</td>
                          <td className="py-1.5 px-2 text-right">
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={() =>
                                dropSessionMut.mutate({
                                  sessionId: s.id,
                                  clientId: s.clientId ?? undefined,
                                  ip: s.sourceIp,
                                })
                              }
                            >
                              <Trash2 className="h-3.5 w-3.5 text-red-400" /> {isRu ? 'Сбросить' : 'Drop'}
                            </Button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="xray-config">
          <XrayConfigStudioPanel nodeId={id} nodeName={node?.name} />
        </TabsContent>

        <TabsContent value="clients">
          <Card>
            <CardHeader className="flex flex-wrap flex-row items-center justify-between gap-3">
              <CardTitle className="flex items-center gap-2">
                <Users className="h-4 w-4 text-primary" />
                {isRu ? `Клиенты на ноде ${node?.name || id}` : `Clients on ${node?.name || id}`}
              </CardTitle>
              <div className="flex flex-wrap items-center gap-2">
                <div className="relative w-52">
                  <Search className="absolute left-2.5 top-2 h-3.5 w-3.5 text-muted-foreground" />
                  <Input
                    value={clientSearch}
                    onChange={(e) => setClientSearch(e.target.value)}
                    placeholder={t('common.search')}
                    className="h-8 pl-8 text-xs"
                  />
                </div>
                <Button
                  size="sm"
                  variant={pinnedOnly ? 'default' : 'outline'}
                  onClick={() => setPinnedOnly((v) => !v)}
                >
                  {pinnedOnly ? t('nodes.pinnedFilter') : t('nodes.allActiveClients')}
                </Button>
                <Button size="sm" variant="outline" asChild>
                  <Link to="/clients">{t('nodes.manageInClients')}</Link>
                </Button>
              </div>
            </CardHeader>
            <CardContent>
              {clientsLoading ? (
                <div className="py-6 text-center text-xs text-muted-foreground">{t('common.loading')}</div>
              ) : filteredNodeClients.length === 0 ? (
                <div className="py-6 text-center text-xs text-muted-foreground">{t('common.noData')}</div>
              ) : (
                <div className="overflow-x-auto">
                  <table className="w-full text-xs">
                    <thead>
                      <tr className="border-b text-left text-muted-foreground">
                        <th className="py-1.5 px-2">{t('auth.username')}</th>
                        <th className="py-1.5 px-2">{isRu ? 'Привязка' : 'Scope'}</th>
                        <th className="py-1.5 px-2">{t('clients.colProtocols')}</th>
                        <th className="py-1.5 px-2">{t('clients.colTraffic')}</th>
                        <th className="py-1.5 px-2">{t('common.status')}</th>
                        <th className="py-1.5 px-2 text-right">{t('common.actions')}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {filteredNodeClients.map((c) => (
                        <tr key={c.id} className="border-b last:border-b-0">
                          <td className="py-1.5 px-2">
                            <div className="font-medium">{c.username}</div>
                            <div className="font-mono text-[11px] text-muted-foreground">
                              {c.uuid.slice(0, 8)}...
                            </div>
                          </td>
                          <td className="py-1.5 px-2">
                            {c.allNodes !== false ? (
                              <Badge variant="outline" className="text-[10px]">
                                {t('clients.allNodesBadge')}
                              </Badge>
                            ) : (
                              <Badge variant="secondary" className="text-[10px]">
                                {isRu ? 'Привязан' : 'Pinned'}
                              </Badge>
                            )}
                          </td>
                          <td className="py-1.5 px-2">
                            <div className="flex flex-wrap gap-1">
                              {(c.protocols ?? []).map((p) => (
                                <Badge key={p} className={cn('border text-[10px]', PROTOCOL_COLORS[p])}>
                                  {p}
                                </Badge>
                              ))}
                            </div>
                          </td>
                          <td className="py-1.5 px-2 font-mono tabular-nums">
                            {formatBytes(c.usedTraffic || 0, { locale: language })} /{' '}
                            {c.trafficLimit > 0 ? formatBytes(c.trafficLimit, { locale: language }) : '∞'}
                          </td>
                          <td className="py-1.5 px-2">
                            <Badge variant={c.banned ? 'destructive' : c.onHold ? 'warning' : 'success'}>
                              {c.banned
                                ? t('common.banned')
                                : c.onHold
                                  ? t('common.onHold')
                                  : t('common.active')}
                            </Badge>
                          </td>
                          <td className="py-1.5 px-2 text-right">
                            <Button
                              size="sm"
                              variant="ghost"
                              disabled={unbindClientMut.isPending}
                              onClick={() => unbindClientMut.mutate(c.id)}
                              title={t('nodes.unbind')}
                            >
                              <X className="h-3.5 w-3.5 text-amber-400" /> {t('nodes.unbind')}
                            </Button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {/* Install & Pairing Wizard Modal */}
      <NodeInstallModal
        open={installModalOpen}
        onOpenChange={setInstallModalOpen}
        nodeId={id}
        initialData={
          node
            ? {
                nodeId: node.id,
                name: node.name,
                host: node.host,
                port: node.port,
                apiPort: node.apiPort,
              }
            : undefined
        }
        defaultTab="connect"
      />
    </div>
  );
}
