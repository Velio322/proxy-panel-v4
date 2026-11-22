import { useEffect, useMemo, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Activity,
  ArrowRight,
  Clock,
  Edit,
  Layers,
  LayoutGrid,
  List,
  MoreHorizontal,
  Plus,
  RefreshCw,
  Search,
  Server,
  Signal,
  Sparkles,
  Terminal,
  Trash2,
  UploadCloud,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { NodeBladeCard } from '@/components/nodes/NodeBladeCard';
import { NodeInstallModal, type NodeInstallInitialData } from '@/components/nodes/NodeInstallModal';
import { MeterBar, formatUptime, latencyBadgeClass } from '@/components/nodes/NodeTelemetryPrimitives';
import { getApiErrorMessage, nodesApi } from '@/lib/api';
import { PROTOCOL_COLORS, cn, formatBytes, generateNodeSecret } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { NodeItem, NodeStatus, Protocol } from '@/types';

function statusBadgeVariant(status: NodeStatus) {
  switch (status) {
    case 'ONLINE':
      return 'success' as const;
    case 'MAINTENANCE':
      return 'warning' as const;
    default:
      return 'destructive' as const;
  }
}

export function NodesPage() {
  const { t } = useTranslation();
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';
  const qc = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();

  const [viewMode, setViewMode] = useState<'blades' | 'table'>('blades');
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [checkingAll, setCheckingAll] = useState(false);
  const [busyAction, setBusyAction] = useState<{ id: string; action: string } | null>(null);

  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<NodeItem | null>(null);
  const [installModal, setInstallModal] = useState<{
    open: boolean;
    nodeId?: string | null;
    initialData?: NodeInstallInitialData | null;
    defaultTab?: 'connect' | 'scenarios';
  }>({ open: false });

  const [name, setName] = useState('');
  const [host, setHost] = useState('');
  const [port, setPort] = useState('443');
  const [apiPort, setApiPort] = useState('2222');
  const [secret, setSecret] = useState('');
  const [country, setCountry] = useState('');
  const [city, setCity] = useState('');
  const [isp, setIsp] = useState('');
  const [tags, setTags] = useState('');
  const [active, setActive] = useState(true);

  const openCreate = () => {
    setEditing(null);
    setName('');
    setHost('');
    setPort('443');
    setApiPort('2222');
    setSecret(generateNodeSecret(24));
    setCountry('');
    setCity('');
    setIsp('');
    setTags('');
    setActive(true);
    setModalOpen(true);
  };

  useEffect(() => {
    if (searchParams.get('create') === '1') {
      openCreate();
      const next = new URLSearchParams(searchParams);
      next.delete('create');
      setSearchParams(next, { replace: true });
    }
  }, [searchParams, setSearchParams]);

  const { data: nodes = [], isLoading } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
    refetchInterval: 15_000,
  });

  const fleetStats = useMemo(() => {
    let online = 0;
    let offline = 0;
    let maintenance = 0;
    let error = 0;
    let totalInbounds = 0;
    let totalUp = 0;
    let totalDown = 0;
    const cpuVals: number[] = [];
    const memVals: number[] = [];
    const pings: number[] = [];
    const protocols = new Set<Protocol>();

    for (const n of nodes) {
      if (n.status === 'ONLINE') online += 1;
      else if (n.status === 'MAINTENANCE') maintenance += 1;
      else if (n.status === 'ERROR') error += 1;
      else offline += 1;

      totalInbounds += n._count?.inbounds ?? n.inbounds?.length ?? 0;
      totalUp += n.netUpload ?? 0;
      totalDown += n.netDownload ?? 0;

      if (n.status === 'ONLINE') {
        if (n.cpuUsage != null && Number.isFinite(n.cpuUsage)) cpuVals.push(n.cpuUsage);
        if (n.memUsage != null && Number.isFinite(n.memUsage)) memVals.push(n.memUsage);
        if (n.lastPingMs != null && Number.isFinite(n.lastPingMs)) pings.push(n.lastPingMs);
      }

      for (const ib of n.inbounds ?? []) {
        protocols.add(ib.protocol);
      }
    }

    const avgCpu = cpuVals.length > 0 ? cpuVals.reduce((a, b) => a + b, 0) / cpuVals.length : null;
    const avgMem = memVals.length > 0 ? memVals.reduce((a, b) => a + b, 0) / memVals.length : null;
    const avgPing = pings.length > 0 ? pings.reduce((a, b) => a + b, 0) / pings.length : null;
    const totalTraffic = totalUp + totalDown;
    const dlShare = totalTraffic > 0 ? Math.round((totalDown / totalTraffic) * 100) : 50;

    return {
      total: nodes.length,
      online,
      offline,
      maintenance,
      error,
      totalInbounds,
      totalUp,
      totalDown,
      totalTraffic,
      dlShare,
      avgCpu,
      avgMem,
      avgPing,
      protocols: Array.from(protocols),
    };
  }, [nodes]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return nodes.filter((n) => {
      if (statusFilter !== 'all' && n.status !== statusFilter) return false;
      if (!q) return true;
      const haystack = [
        n.name,
        n.host,
        n.country ?? '',
        n.city ?? '',
        n.isp ?? '',
        ...(n.tags ?? []),
        ...(n.inbounds ?? []).flatMap((ib) => [ib.protocol, ib.tag, String(ib.port)]),
      ]
        .join(' ')
        .toLowerCase();
      return haystack.includes(q);
    });
  }, [nodes, search, statusFilter]);

  const openEdit = (n: NodeItem) => {
    setEditing(n);
    setName(n.name);
    setHost(n.host);
    setPort(String(n.port));
    setApiPort(String(n.apiPort));
    setSecret('');
    setCountry(n.country ?? '');
    setCity(n.city ?? '');
    setIsp(n.isp ?? '');
    setTags((n.tags ?? []).join(', '));
    setActive(n.active);
    setModalOpen(true);
  };

  const saveMut = useMutation({
    mutationFn: async () => {
      const cleanSecret = secret.trim();
      if (!editing && cleanSecret.length < 16) {
        throw new Error(
          isRu ? 'Секрет ноды — минимум 16 символов' : 'Node secret must be at least 16 characters',
        );
      }
      if (editing && cleanSecret && cleanSecret.length < 16) {
        throw new Error(
          isRu ? 'Секрет ноды — минимум 16 символов' : 'Node secret must be at least 16 characters',
        );
      }
      const payload: Record<string, unknown> = {
        name: name.trim(),
        host: host.trim(),
        port: Number(port) || 443,
        apiPort: Number(apiPort) || 2222,
        country: country.trim() || null,
        city: city.trim() || null,
        isp: isp.trim() || null,
        tags: tags
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
        active,
        ...(cleanSecret ? { secret: cleanSecret } : {}),
      };
      if (editing) return nodesApi.update(editing.id, payload);
      return nodesApi.create(payload);
    },
    onSuccess: async (saved) => {
      qc.invalidateQueries({ queryKey: ['nodes'] });
      setModalOpen(false);
      toast({
        title: editing
          ? isRu
            ? 'Нода обновлена'
            : 'Node updated'
          : isRu
            ? 'Нода добавлена'
            : 'Node created',
        variant: 'success',
      });
      if (!editing && saved?.id) {
        const created = saved as NodeItem & NodeInstallInitialData;
        setInstallModal({
          open: true,
          nodeId: created.id,
          initialData: {
            nodeId: created.id,
            name: created.name,
            host: created.host,
            port: created.port,
            apiPort: created.apiPort,
            secret: created.secret,
            isDomain: created.isDomain,
            panelUrl: created.panelUrl,
            wsUrl: created.wsUrl,
            scriptUrl: created.scriptUrl,
            binaryUrl: created.binaryUrl,
            command: created.command,
            installCommand: created.installCommand,
            setupInstructions: created.setupInstructions,
          },
          defaultTab: 'connect',
        });
      }
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка сохранения' : 'Save failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const actionMut = useMutation({
    mutationFn: async ({ id, action }: { id: string; action: 'check' | 'push' | 'restart' | 'delete' }) => {
      if (action === 'check') return nodesApi.check(id);
      if (action === 'push') return nodesApi.pushConfig(id);
      if (action === 'restart') return nodesApi.restart(id);
      return nodesApi.delete(id);
    },
    onMutate: (vars) => setBusyAction({ id: vars.id, action: vars.action }),
    onSuccess: (_, vars) => {
      qc.invalidateQueries({ queryKey: ['nodes'] });
      const labels: Record<string, string> = isRu
        ? {
            check: 'Проверка ноды завершена',
            push: 'Конфигурация применена',
            restart: 'Ядра перезапущены',
            delete: 'Нода удалена',
          }
        : {
            check: 'Node health check completed',
            push: 'Configuration synced',
            restart: 'Cores restarted',
            delete: 'Node deleted',
          };
      toast({
        title: labels[vars.action] ?? `Node ${vars.action} succeeded`,
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка операции' : 'Action failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
    onSettled: () => setBusyAction(null),
  });

  const handleCheckAll = async () => {
    const activeNodes = filtered.filter((n) => n.active);
    if (activeNodes.length === 0) return;
    setCheckingAll(true);
    try {
      const results = await Promise.allSettled(activeNodes.map((n) => nodesApi.check(n.id)));
      const okCount = results.filter((r) => r.status === 'fulfilled').length;
      qc.invalidateQueries({ queryKey: ['nodes'] });
      toast({
        title: isRu
          ? `Проверено нод: ${okCount} из ${activeNodes.length}`
          : `Checked ${okCount} of ${activeNodes.length} nodes`,
        variant: okCount > 0 ? 'success' : 'destructive',
      });
    } finally {
      setCheckingAll(false);
    }
  };

  const openInstallCmd = (node: NodeItem) => {
    setInstallModal({
      open: true,
      nodeId: node.id,
      initialData: {
        nodeId: node.id,
        name: node.name,
        host: node.host,
        port: node.port,
        apiPort: node.apiPort,
      },
      defaultTab: 'connect',
    });
  };

  const statusTabs: Array<{ key: string; label: string; count: number }> = [
    { key: 'all', label: isRu ? 'Все' : 'All', count: fleetStats.total },
    { key: 'ONLINE', label: isRu ? 'Онлайн' : 'Online', count: fleetStats.online },
    { key: 'OFFLINE', label: isRu ? 'Офлайн' : 'Offline', count: fleetStats.offline },
    { key: 'MAINTENANCE', label: isRu ? 'Обслуживание' : 'Maintenance', count: fleetStats.maintenance },
    { key: 'ERROR', label: isRu ? 'Ошибки' : 'Error', count: fleetStats.error },
  ];

  return (
    <div className="mx-auto w-full max-w-7xl space-y-4">
      {/* Page Header */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className="flex items-center gap-2.5">
            <h1 className="text-2xl font-bold tracking-tight">{t('nodes.title')}</h1>
            <Badge variant="outline" className="font-mono text-xs tabular-nums">
              {fleetStats.online}/{fleetStats.total} {isRu ? 'онлайн' : 'online'}
            </Badge>
          </div>
          <p className="mt-0.5 text-xs text-muted-foreground">{t('nodes.subtitle')}</p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setInstallModal({ open: true, defaultTab: 'scenarios' })}
          >
            <Layers className="h-3.5 w-3.5 text-primary" />
            <span>{isRu ? 'Установка (4 режима)' : 'Setup Guide'}</span>
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={handleCheckAll}
            loading={checkingAll}
            disabled={nodes.length === 0}
          >
            <Activity className="h-3.5 w-3.5 text-primary" />
            <span>{isRu ? 'Проверить все' : 'Check All'}</span>
          </Button>
          <div className="inline-flex rounded-lg border border-border/80 bg-muted/40 p-0.5">
            <Button
              type="button"
              size="sm"
              variant={viewMode === 'blades' ? 'secondary' : 'ghost'}
              className="h-7 gap-1 px-2.5 text-xs"
              onClick={() => setViewMode('blades')}
            >
              <LayoutGrid className="h-3.5 w-3.5" />
              <span>{isRu ? 'Карточки' : 'Blades'}</span>
            </Button>
            <Button
              type="button"
              size="sm"
              variant={viewMode === 'table' ? 'secondary' : 'ghost'}
              className="h-7 gap-1 px-2.5 text-xs"
              onClick={() => setViewMode('table')}
            >
              <List className="h-3.5 w-3.5" />
              <span>{isRu ? 'Таблица' : 'Table'}</span>
            </Button>
          </div>
          <Button onClick={openCreate}>
            <Plus className="h-4 w-4" />
            {t('nodes.addNode')}
          </Button>
        </div>
      </div>

      {/* Fleet Telemetry Ribbon */}
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Card className="p-3.5 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Флот нод' : 'Node Fleet'}</span>
            <Server className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="flex items-baseline justify-between">
            <span className="font-mono text-2xl font-bold tabular-nums">
              {fleetStats.online} / {fleetStats.total}
            </span>
            <span className="text-xs text-muted-foreground tabular-nums">
              {fleetStats.totalInbounds} {isRu ? 'протоколов' : 'inbounds'}
            </span>
          </div>
          <div className="telemetry-track flex h-1.5 w-full overflow-hidden rounded-full">
            {fleetStats.total > 0 && (
              <>
                <div
                  className="h-full bg-emerald-400"
                  style={{ width: `${(fleetStats.online / fleetStats.total) * 100}%` }}
                />
                <div
                  className="h-full bg-amber-400"
                  style={{ width: `${(fleetStats.maintenance / fleetStats.total) * 100}%` }}
                />
                <div
                  className="h-full bg-red-500"
                  style={{ width: `${(fleetStats.error / fleetStats.total) * 100}%` }}
                />
              </>
            )}
          </div>
        </Card>

        <Card className="p-3.5 space-y-1.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Средняя нагрузка' : 'Average Load'}</span>
            <span className="font-mono text-[11px]">{isRu ? 'онлайн-ноды' : 'online nodes'}</span>
          </div>
          <div className="space-y-1 text-xs">
            <div className="flex items-center justify-between font-mono tabular-nums">
              <span className="text-muted-foreground">CPU</span>
              <span className="font-semibold">
                {fleetStats.avgCpu != null ? `${fleetStats.avgCpu.toFixed(1)}%` : '—'}
              </span>
            </div>
            <MeterBar value={fleetStats.avgCpu} tone="emerald" />
            <div className="flex items-center justify-between font-mono tabular-nums pt-0.5">
              <span className="text-muted-foreground">RAM</span>
              <span className="font-semibold">
                {fleetStats.avgMem != null ? `${fleetStats.avgMem.toFixed(1)}%` : '—'}
              </span>
            </div>
            <MeterBar value={fleetStats.avgMem} tone="cyan" />
          </div>
        </Card>

        <Card className="p-3.5 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Суммарный трафик' : 'Fleet Traffic'}</span>
            <span className="font-mono text-[11px]">↓/↑</span>
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {formatBytes(fleetStats.totalTraffic, { locale: language })}
          </div>
          <div className="space-y-1">
            <div className="flex items-center justify-between font-mono text-[11px] tabular-nums">
              <span className="text-emerald-400">
                ↓ {formatBytes(fleetStats.totalDown, { locale: language })}
              </span>
              <span className="text-cyan-400">
                ↑ {formatBytes(fleetStats.totalUp, { locale: language })}
              </span>
            </div>
            <div className="telemetry-track flex h-1.5 w-full overflow-hidden rounded-full">
              <div className="h-full bg-emerald-400" style={{ width: `${fleetStats.dlShare}%` }} />
              <div className="h-full bg-cyan-400" style={{ width: `${100 - fleetStats.dlShare}%` }} />
            </div>
          </div>
        </Card>

        <Card className="p-3.5 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Задержка и протоколы' : 'Latency & Protocols'}</span>
            <Signal className="h-4 w-4 text-cyan-400" />
          </div>
          <div className="flex items-baseline justify-between">
            <span className="font-mono text-2xl font-bold tabular-nums">
              {fleetStats.avgPing != null ? `${Math.round(fleetStats.avgPing)} ms` : '—'}
            </span>
            <span className="text-xs text-muted-foreground">{isRu ? 'ср. пинг' : 'avg ping'}</span>
          </div>
          <div className="flex flex-wrap gap-1">
            {fleetStats.protocols.length === 0 ? (
              <span className="text-xs text-muted-foreground">{isRu ? 'Нет протоколов' : 'No protocols'}</span>
            ) : (
              fleetStats.protocols.slice(0, 5).map((p) => (
                <Badge key={p} className={cn('border px-1.5 py-0 text-[10px]', PROTOCOL_COLORS[p])}>
                  {p}
                </Badge>
              ))
            )}
          </div>
        </Card>
      </div>

      {/* Filter & Search Toolbar */}
      <Card>
        <CardHeader className="py-2.5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex flex-wrap items-center gap-1.5">
              {statusTabs.map((tab) => (
                <button
                  key={tab.key}
                  type="button"
                  onClick={() => setStatusFilter(tab.key)}
                  className={cn(
                    'inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-xs font-medium transition-colors',
                    statusFilter === tab.key
                      ? 'border-primary/45 bg-primary/15 text-primary'
                      : 'border-border/70 bg-background/40 text-muted-foreground hover:border-border hover:text-foreground',
                  )}
                >
                  <span>{tab.label}</span>
                  <span className="font-mono text-[10px] opacity-80 tabular-nums">{tab.count}</span>
                </button>
              ))}
            </div>

            <div className="relative w-full sm:w-72">
              <Search className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
              <Input
                placeholder={isRu ? 'Поиск ноды, хоста, страны, протокола...' : t('common.search')}
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                className="pl-9"
              />
            </div>
          </div>
        </CardHeader>
      </Card>

      {/* Main Content */}
      {isLoading ? (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          <Skeleton className="h-64 w-full rounded-xl" />
          <Skeleton className="h-64 w-full rounded-xl" />
        </div>
      ) : filtered.length === 0 ? (
        <Card className="p-10 text-center">
          <div className="mx-auto max-w-sm space-y-3">
            <Server className="mx-auto h-9 w-9 text-muted-foreground/60" />
            <div className="text-sm font-medium">{t('common.noData')}</div>
            <p className="text-xs text-muted-foreground">
              {nodes.length === 0
                ? isRu
                  ? 'Добавьте первую серверную ноду для запуска прокси-протоколов.'
                  : 'Add your first proxy server node to start serving protocols.'
                : isRu
                  ? 'Попробуйте сбросить фильтры или поисковый запрос.'
                  : 'Try adjusting your filters or search query.'}
            </p>
            {nodes.length === 0 && (
              <Button size="sm" onClick={openCreate}>
                <Plus className="h-4 w-4" />
                {t('nodes.addNode')}
              </Button>
            )}
          </div>
        </Card>
      ) : viewMode === 'blades' ? (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          {filtered.map((n) => (
            <NodeBladeCard
              key={n.id}
              node={n}
              isRu={isRu}
              language={language}
              busyAction={busyAction?.id === n.id ? busyAction.action : null}
              onAction={(id, action) => actionMut.mutate({ id, action })}
              onEdit={openEdit}
              onOpenInstall={openInstallCmd}
            />
          ))}
        </div>
      ) : (
        <Card>
          <CardContent className="p-0">
            <div className="overflow-x-auto">
              <table className="data-table w-full text-sm">
                <thead className="bg-muted/40">
                  <tr className="border-b border-border/80 text-left text-xs text-muted-foreground">
                    <th className="py-2.5 px-3">{isRu ? 'Нода' : 'Node'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Адрес / Пинг' : 'Endpoint / Ping'}</th>
                    <th className="py-2.5 px-3">{t('common.status')}</th>
                    <th className="py-2.5 px-3">CPU / RAM</th>
                    <th className="py-2.5 px-3">{isRu ? 'Трафик' : 'Traffic'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Протоколы' : 'Protocols'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Ядра' : 'Cores'}</th>
                    <th className="py-2.5 px-3 text-right">{t('common.actions')}</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((n) => {
                    const rowUptime = n.uptime ?? n.nodeMetrics?.[0]?.uptime ?? null;
                    return (
                    <tr key={n.id} className="border-b border-border/60 last:border-b-0 hover:bg-accent/35">
                      <td className="py-2.5 px-3 font-medium">
                        <Link to={`/nodes/${n.id}`} className="hover:text-primary flex items-center gap-2">
                          <span
                            className={cn(
                              'h-2 w-2 shrink-0 rounded-full',
                              n.status === 'ONLINE'
                                ? 'bg-emerald-400 animate-pulse'
                                : n.status === 'MAINTENANCE'
                                  ? 'bg-amber-400'
                                  : 'bg-red-500',
                            )}
                          />
                          <span>{n.name}</span>
                        </Link>
                      </td>
                      <td className="py-2.5 px-3 font-mono text-xs space-y-1">
                        <div>
                          {n.host}:{n.port}
                        </div>
                        <div className="flex flex-wrap items-center gap-1">
                          <span
                            className={cn(
                              'inline-flex items-center gap-1 rounded border px-1.5 py-0 text-[10px]',
                              latencyBadgeClass(n.lastPingMs),
                            )}
                          >
                            <Signal className="h-2.5 w-2.5" />
                            {n.lastPingMs != null ? `${n.lastPingMs} ms` : '—'}
                          </span>
                          <span className="inline-flex items-center gap-1 rounded border border-border/70 bg-muted/40 px-1.5 py-0 text-[10px] text-muted-foreground">
                            <Clock className="h-2.5 w-2.5" />
                            {n.status === 'ONLINE' && rowUptime != null
                              ? `↑ ${formatUptime(rowUptime, isRu)}`
                              : '—'}
                          </span>
                        </div>
                      </td>
                      <td className="py-2.5 px-3">
                        <Badge variant={statusBadgeVariant(n.status)}>{n.status}</Badge>
                      </td>
                      <td className="py-2.5 px-3 text-xs tabular-nums min-w-[140px]">
                        <div className="space-y-1">
                          <div className="flex items-center gap-2">
                            <span className="w-7 text-[10px] text-muted-foreground">CPU</span>
                            <MeterBar value={n.cpuUsage} tone="emerald" className="h-1 flex-1" />
                            <span className="w-10 text-right font-mono">
                              {n.cpuUsage != null ? `${n.cpuUsage.toFixed(0)}%` : '—'}
                            </span>
                          </div>
                          <div className="flex items-center gap-2">
                            <span className="w-7 text-[10px] text-muted-foreground">RAM</span>
                            <MeterBar value={n.memUsage} tone="cyan" className="h-1 flex-1" />
                            <span className="w-10 text-right font-mono">
                              {n.memUsage != null ? `${n.memUsage.toFixed(0)}%` : '—'}
                            </span>
                          </div>
                        </div>
                      </td>
                      <td className="py-2.5 px-3 font-mono text-xs tabular-nums">
                        <div className="text-emerald-400">
                          ↓ {formatBytes(n.netDownload ?? 0, { locale: language })}
                        </div>
                        <div className="text-cyan-400">
                          ↑ {formatBytes(n.netUpload ?? 0, { locale: language })}
                        </div>
                      </td>
                      <td className="py-2.5 px-3">
                        <div className="flex flex-wrap gap-1 max-w-[200px]">
                          {(n.inbounds ?? []).map((ib) => (
                            <Badge
                              key={ib.id}
                              className={cn('border px-1.5 py-0 text-[10px]', PROTOCOL_COLORS[ib.protocol])}
                            >
                              {ib.protocol}:{ib.port}
                            </Badge>
                          ))}
                          {(n.inbounds ?? []).length === 0 && (
                            <span className="text-xs text-muted-foreground">—</span>
                          )}
                        </div>
                      </td>
                      <td className="py-2.5 px-3 font-mono text-[11px] text-muted-foreground">
                        {[
                          n.xrayVersion && `Xray ${n.xrayVersion}`,
                          n.singboxVersion && `SB ${n.singboxVersion}`,
                          n.naiveVersion && `Naive ${n.naiveVersion}`,
                          n.mieruVersion && `Mieru ${n.mieruVersion}`,
                        ]
                          .filter(Boolean)
                          .join(' · ') || '—'}
                      </td>
                      <td className="py-2.5 px-3 text-right">
                        <div className="inline-flex items-center gap-1.5">
                          <Button size="sm" variant="secondary" asChild className="h-8 gap-1 px-2.5 text-xs">
                            <Link to={`/nodes/${n.id}`}>
                              <span>{t('common.open')}</span>
                              <ArrowRight className="h-3.5 w-3.5" />
                            </Link>
                          </Button>
                          <Button
                            size="sm"
                            variant="outline"
                            className="h-8 gap-1 px-2.5 text-xs"
                            loading={busyAction?.id === n.id && busyAction.action === 'push'}
                            onClick={() => actionMut.mutate({ id: n.id, action: 'push' })}
                          >
                            <UploadCloud className="h-3.5 w-3.5" />
                            <span>{t('nodes.sync')}</span>
                          </Button>

                          <DropdownMenu>
                            <DropdownMenuTrigger asChild>
                              <Button
                                size="icon"
                                variant="ghost"
                                className="h-8 w-8"
                                aria-label={t('common.actions')}
                              >
                                <MoreHorizontal className="h-4 w-4" />
                              </Button>
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="end" className="w-52">
                              <DropdownMenuItem onClick={() => openInstallCmd(n)}>
                                <Terminal className="h-3.5 w-3.5" />
                                <span>{t('nodes.installCmd')}</span>
                              </DropdownMenuItem>
                              <DropdownMenuItem onClick={() => actionMut.mutate({ id: n.id, action: 'check' })}>
                                <Activity className="h-3.5 w-3.5 text-primary" />
                                <span>{t('nodes.healthCheck')}</span>
                              </DropdownMenuItem>
                              <DropdownMenuItem
                                onClick={() => actionMut.mutate({ id: n.id, action: 'restart' })}
                              >
                                <RefreshCw className="h-3.5 w-3.5" />
                                <span>{t('nodes.restartCores')}</span>
                              </DropdownMenuItem>
                              <DropdownMenuItem onClick={() => openEdit(n)}>
                                <Edit className="h-3.5 w-3.5" />
                                <span>{t('common.edit')}</span>
                              </DropdownMenuItem>
                              <DropdownMenuSeparator />
                              <DropdownMenuItem
                                onClick={() => actionMut.mutate({ id: n.id, action: 'delete' })}
                                className="text-red-500 dark:text-red-400 focus:text-red-500 dark:focus:text-red-400"
                              >
                                <Trash2 className="h-3.5 w-3.5" />
                                <span>{t('nodes.deleteNode')}</span>
                              </DropdownMenuItem>
                            </DropdownMenuContent>
                          </DropdownMenu>
                        </div>
                      </td>
                    </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Create / Edit Modal */}
      <Dialog open={modalOpen} onOpenChange={setModalOpen}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>
              {editing
                ? isRu
                  ? `Редактировать ноду: ${editing.name}`
                  : `Edit Node: ${editing.name}`
                : t('nodes.addNode')}
            </DialogTitle>
            <DialogDescription>
              {editing
                ? isRu
                  ? 'Обновите адрес, порты и метаданные сервера.'
                  : 'Update node endpoint, ports, and location metadata.'
                : isRu
                  ? 'Сервер может быть ещё пустым — после сохранения вы получите команду установки и автоматического подключения по WSS.'
                  : 'The VPS can still be empty — saving generates a 1-line install & WSS pairing command.'}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Название' : 'Name'}</Label>
                <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="EU-Frankfurt-1" />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Домен / IP' : 'Host / Domain'}</Label>
                <Input value={host} onChange={(e) => setHost(e.target.value)} placeholder="node1.example.com" />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Основной порт' : 'Proxy Port'}</Label>
                <Input type="number" value={port} onChange={(e) => setPort(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Порт агента (API)' : 'Worker API Port'}</Label>
                <Input type="number" placeholder="2222" value={apiPort} onChange={(e) => setApiPort(e.target.value)} />
              </div>
            </div>
            <div className="space-y-1.5">
              <div className="flex items-center justify-between">
                <Label>{isRu ? 'Секрет ноды (мин. 16 симв.)' : 'Node Secret (min 16 chars)'}</Label>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="h-6 px-2 text-xs"
                  onClick={() => setSecret(generateNodeSecret(24))}
                >
                  <Sparkles className="h-3 w-3" /> {isRu ? 'Сгенерировать' : 'Generate'}
                </Button>
              </div>
              <Input
                value={secret}
                onChange={(e) => setSecret(e.target.value)}
                placeholder={
                  editing
                    ? isRu
                      ? 'Оставьте пустым, чтобы не менять'
                      : 'Leave blank to keep existing secret'
                    : isRu
                      ? 'Минимум 16 символов'
                      : 'Min 16 characters'
                }
                className="font-mono text-xs"
              />
            </div>
            <div className="grid gap-3 sm:grid-cols-3">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Страна' : 'Country'}</Label>
                <Input value={country} onChange={(e) => setCountry(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Город' : 'City'}</Label>
                <Input value={city} onChange={(e) => setCity(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Провайдер (ISP)' : 'ISP'}</Label>
                <Input value={isp} onChange={(e) => setIsp(e.target.value)} />
              </div>
            </div>
            <div className="space-y-1.5">
              <Label>{isRu ? 'Теги (через запятую)' : 'Tags (comma-separated)'}</Label>
              <Input value={tags} onChange={(e) => setTags(e.target.value)} placeholder="eu, reality" />
            </div>
            <div className="flex items-center justify-between rounded-lg border px-3 py-2">
              <Label>{isRu ? 'Активна' : 'Active'}</Label>
              <Switch checked={active} onCheckedChange={setActive} />
            </div>
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setModalOpen(false)}>
              {t('common.cancel')}
            </Button>
            <Button
              type="button"
              onClick={() => saveMut.mutate()}
              loading={saveMut.isPending}
              disabled={!name.trim() || !host.trim()}
            >
              {t('common.save')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Install & Pairing Wizard Modal */}
      <NodeInstallModal
        open={installModal.open}
        onOpenChange={(o) => !o && setInstallModal({ open: false })}
        nodeId={installModal.nodeId}
        initialData={installModal.initialData}
        defaultTab={installModal.defaultTab}
      />
    </div>
  );
}
