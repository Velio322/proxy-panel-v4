import { useMemo, useState } from 'react';
import { Link } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
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
  ArrowRight,
  Clock,
  Cpu,
  LayoutDashboard,
  Network,
  Plus,
  ScrollText,
  Server,
  Signal,
  Users,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { PageHero, RackEmptyState, SplitBar } from '@/components/ui/PagePrimitives';
import {
  MeterBar,
  RadialGauge,
  formatUptime,
  latencyBadgeClass,
} from '@/components/nodes/NodeTelemetryPrimitives';
import { dashboardApi, monitoringApi, nodesApi } from '@/lib/api';
import {
  PROTOCOL_COLORS,
  cn,
  formatBps,
  formatBytes,
  formatDate,
  formatDateTime,
} from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useUiStore } from '@/lib/store';
import type { Protocol } from '@/types';

function auditBadgeVariant(action: string) {
  const up = action.toUpperCase();
  if (up.includes('CREATE') || up.includes('ADD') || up.includes('ENABLE')) return 'success' as const;
  if (up.includes('DELETE') || up.includes('BAN') || up.includes('DROP')) return 'destructive' as const;
  if (up.includes('UPDATE') || up.includes('PUSH') || up.includes('RESTART')) return 'default' as const;
  return 'secondary' as const;
}

export function DashboardPage() {
  const { t } = useTranslation();
  const timezone = useUiStore((s) => s.timezone);
  const language = useUiStore((s) => s.language);
  const [days, setDays] = useState(14);
  const isRu = language === 'ru';

  const { data: overview, isLoading: overviewLoading } = useQuery({
    queryKey: ['dashboard-overview'],
    queryFn: () => dashboardApi.overview(),
    refetchInterval: 15_000,
  });

  const { data: nodes = [], isLoading: nodesLoading } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
    refetchInterval: 15_000,
  });

  const { data: chart = [] } = useQuery({
    queryKey: ['dashboard-chart', days],
    queryFn: () => dashboardApi.trafficChart(days),
  });

  const { data: topClients = [] } = useQuery({
    queryKey: ['dashboard-top-clients'],
    queryFn: () => dashboardApi.topClients(7, 6),
  });

  const { data: recentAudit = [] } = useQuery({
    queryKey: ['dashboard-recent-audit'],
    queryFn: () => dashboardApi.recentAudit(6),
    retry: false,
  });

  const { data: hostTelemetry } = useQuery({
    queryKey: ['monitoring-host'],
    queryFn: () => monitoringApi.host(),
    refetchInterval: 10_000,
    retry: false,
  });

  const chartData = chart.map((pt) => ({
    day: formatDate(pt.day, { month: 'short', day: 'numeric' }, timezone),
    upload: Number(pt.upload || 0),
    download: Number(pt.download || 0),
  }));

  const periodTotals = useMemo(() => {
    let ul = 0;
    let dl = 0;
    for (const pt of chartData) {
      ul += pt.upload;
      dl += pt.download;
    }
    return { ul, dl, total: ul + dl };
  }, [chartData]);

  const activeProtocols = useMemo(() => {
    const set = new Set<Protocol>();
    for (const n of nodes) {
      for (const ib of n.inbounds ?? []) {
        if (ib.enable) set.add(ib.protocol);
      }
    }
    return Array.from(set);
  }, [nodes]);

  const cpuPct =
    hostTelemetry && hostTelemetry.cpu.usagePercent >= 0
      ? Math.min(100, Math.max(0, hostTelemetry.cpu.usagePercent))
      : null;
  const memPct =
    hostTelemetry && hostTelemetry.memory.usagePercent >= 0
      ? Math.min(100, Math.max(0, hostTelemetry.memory.usagePercent))
      : null;

  const clientsTotal = overview?.clients.total ?? 0;
  const clientsActivePct = clientsTotal > 0 ? ((overview?.clients.active ?? 0) / clientsTotal) * 100 : 0;
  const clientsBannedPct = clientsTotal > 0 ? ((overview?.clients.banned ?? 0) / clientsTotal) * 100 : 0;

  const nodesTotal = overview?.nodes.total ?? 0;
  const nodesOnlinePct = nodesTotal > 0 ? ((overview?.nodes.online ?? 0) / nodesTotal) * 100 : 0;
  const nodesOfflinePct = nodesTotal > 0 ? ((overview?.nodes.offline ?? 0) / nodesTotal) * 100 : 0;

  const monthUl = overview?.traffic.month.upload ?? 0;
  const monthDl = overview?.traffic.month.download ?? 0;
  const monthTotal = monthUl + monthDl;
  const monthDlPct = monthTotal > 0 ? Math.round((monthDl / monthTotal) * 100) : 50;

  const maxTopClientBytes = useMemo(() => {
    let max = 0;
    for (const tc of topClients) {
      const sum = Number(tc._sum.upload || 0) + Number(tc._sum.download || 0);
      if (sum > max) max = sum;
    }
    return max;
  }, [topClients]);

  return (
    <div className="space-y-5">
      <PageHero
        title={t('dashboard.title')}
        subtitle={t('dashboard.subtitle')}
        icon={<LayoutDashboard className="h-4 w-4" />}
        badge={
          overview?.onlineUsers !== undefined ? (
            <Badge variant="success" className="gap-1.5 tabular-nums">
              <span className="h-2 w-2 animate-pulse rounded-full bg-emerald-400" />
              {t('dashboard.onlineBadge', { count: overview.onlineUsers })}
            </Badge>
          ) : undefined
        }
        actions={
          <>
            <Button size="sm" asChild>
              <Link to="/clients?create=1">
                <Users className="h-3.5 w-3.5" />
                {t('dashboard.newClient')}
              </Link>
            </Button>
            <Button size="sm" variant="outline" asChild>
              <Link to="/nodes?create=1">
                <Server className="h-3.5 w-3.5" />
                {t('dashboard.newNode')}
              </Link>
            </Button>
            <Button size="sm" variant="outline" asChild>
              <Link to="/inbounds?create=1">
                <Network className="h-3.5 w-3.5" />
                {t('dashboard.newProtocol')}
              </Link>
            </Button>
          </>
        }
        meta={
          <div className="flex flex-wrap items-center justify-between gap-3 text-xs text-muted-foreground">
            <div className="flex flex-wrap items-center gap-3 font-mono tabular-nums">
              <span>
                {isRu ? 'Аптайм хоста:' : 'Host uptime:'}{' '}
                <strong className="text-foreground">
                  {formatUptime(hostTelemetry?.uptime.hostSeconds, isRu)}
                </strong>
              </span>
              <span className="text-border">·</span>
              <span>
                TCP/UDP:{' '}
                <strong className="text-foreground">
                  {hostTelemetry?.connections.establishedTcp ?? '—'} /{' '}
                  {hostTelemetry?.connections.udpSockets ?? '—'}
                </strong>
              </span>
              <span className="text-border">·</span>
              <span>
                <span className="text-emerald-400">↓ {formatBps(hostTelemetry?.network.downloadBps, language)}</span>{' '}
                · <span className="text-cyan-400">↑ {formatBps(hostTelemetry?.network.uploadBps, language)}</span>
              </span>
            </div>
            <Link to="/monitoring" className="inline-flex items-center gap-1 font-medium text-primary hover:underline">
              <span>{isRu ? 'Телеметрия системы' : 'System Telemetry'}</span>
              <ArrowRight className="h-3.5 w-3.5" />
            </Link>
          </div>
        }
      />

      {/* Clickable 4-Card Tactical KPI Ribbon */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Link to="/clients" className="group block">
          <div className="node-blade relative h-full overflow-hidden rounded-xl border border-border/80 p-4 pl-5 transition-all group-hover:border-primary/45">
            <div className="absolute inset-y-0 left-0 w-1 bg-emerald-400" aria-hidden="true" />
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>{t('dashboard.totalClients')}</span>
              <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-primary/15 text-primary">
                <Users className="h-3.5 w-3.5" />
              </span>
            </div>
            {overviewLoading ? (
              <Skeleton className="mt-2 h-8 w-24" />
            ) : (
              <div className="mt-2 space-y-2.5">
                <div className="flex items-baseline justify-between">
                  <span className="font-mono text-2xl font-bold tabular-nums">{clientsTotal}</span>
                  <ArrowRight className="h-3.5 w-3.5 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />
                </div>
                <SplitBar
                  segments={[
                    { pct: clientsActivePct, className: 'bg-emerald-400' },
                    { pct: clientsBannedPct, className: 'bg-red-500' },
                  ]}
                />
                <p className="text-[11px] text-muted-foreground tabular-nums">
                  {t('dashboard.clientsSub', {
                    active: overview?.clients.active ?? 0,
                    banned: overview?.clients.banned ?? 0,
                  })}
                </p>
              </div>
            )}
          </div>
        </Link>

        <Link to="/nodes" className="group block">
          <div className="node-blade relative h-full overflow-hidden rounded-xl border border-border/80 p-4 pl-5 transition-all group-hover:border-emerald-500/45">
            <div className="absolute inset-y-0 left-0 w-1 bg-emerald-400 shadow-[0_0_10px_hsl(162_84%_43%/0.6)]" aria-hidden="true" />
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>{t('dashboard.onlineNodes')}</span>
              <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-emerald-500/15 text-emerald-400">
                <Server className="h-3.5 w-3.5" />
              </span>
            </div>
            {overviewLoading ? (
              <Skeleton className="mt-2 h-8 w-24" />
            ) : (
              <div className="mt-2 space-y-2.5">
                <div className="flex items-baseline justify-between">
                  <span className="font-mono text-2xl font-bold tabular-nums">
                    {overview?.nodes.online ?? 0} / {nodesTotal}
                  </span>
                  <ArrowRight className="h-3.5 w-3.5 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />
                </div>
                <SplitBar
                  segments={[
                    { pct: nodesOnlinePct, className: 'bg-emerald-400' },
                    { pct: nodesOfflinePct, className: 'bg-zinc-500' },
                  ]}
                />
                <p className="text-[11px] text-muted-foreground tabular-nums">
                  {t('dashboard.nodesSub', { offline: overview?.nodes.offline ?? 0 })}
                </p>
              </div>
            )}
          </div>
        </Link>

        <Link to="/inbounds" className="group block">
          <div className="node-blade relative h-full overflow-hidden rounded-xl border border-border/80 p-4 pl-5 transition-all group-hover:border-cyan-500/45">
            <div className="absolute inset-y-0 left-0 w-1 bg-cyan-400" aria-hidden="true" />
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>{t('dashboard.totalInbounds')}</span>
              <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-cyan-500/15 text-cyan-400">
                <Network className="h-3.5 w-3.5" />
              </span>
            </div>
            {overviewLoading ? (
              <Skeleton className="mt-2 h-8 w-24" />
            ) : (
              <div className="mt-2 space-y-2">
                <div className="flex items-baseline justify-between">
                  <span className="font-mono text-2xl font-bold tabular-nums">{overview?.inbounds.total ?? 0}</span>
                  <span className="text-[11px] text-muted-foreground tabular-nums">
                    {t('dashboard.expiringToday')}: <strong className="text-foreground">{overview?.expiringToday ?? 0}</strong>
                  </span>
                </div>
                <div className="flex flex-wrap gap-1 pt-0.5">
                  {activeProtocols.length === 0 ? (
                    <span className="text-[11px] text-muted-foreground">
                      {isRu ? 'Нет активных протоколов' : 'No active protocols'}
                    </span>
                  ) : (
                    activeProtocols.slice(0, 4).map((p) => (
                      <Badge key={p} className={cn('border px-1.5 py-0 text-[10px]', PROTOCOL_COLORS[p])}>
                        {p}
                      </Badge>
                    ))
                  )}
                </div>
              </div>
            )}
          </div>
        </Link>

        <Link to="/monitoring" className="group block">
          <div className="node-blade relative h-full overflow-hidden rounded-xl border border-border/80 p-4 pl-5 transition-all group-hover:border-teal-500/45">
            <div className="absolute inset-y-0 left-0 w-1 bg-teal-400" aria-hidden="true" />
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>{t('dashboard.trafficMonth')}</span>
              <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-teal-500/15 text-teal-400">
                <Activity className="h-3.5 w-3.5" />
              </span>
            </div>
            {overviewLoading ? (
              <Skeleton className="mt-2 h-8 w-28" />
            ) : (
              <div className="mt-2 space-y-2.5">
                <div className="flex items-baseline justify-between">
                  <span className="font-mono text-2xl font-bold tabular-nums">
                    {formatBytes(monthTotal, { locale: language })}
                  </span>
                  <ArrowRight className="h-3.5 w-3.5 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />
                </div>
                <SplitBar
                  segments={[
                    { pct: monthDlPct, className: 'bg-emerald-400' },
                    { pct: 100 - monthDlPct, className: 'bg-cyan-400' },
                  ]}
                />
                <p className="text-[11px] text-muted-foreground tabular-nums">
                  {t('dashboard.trafficToday')}:{' '}
                  <strong className="text-foreground">
                    {formatBytes(
                      (overview?.traffic.today.upload ?? 0) + (overview?.traffic.today.download ?? 0),
                      { locale: language },
                    )}
                  </strong>
                </p>
              </div>
            )}
          </div>
        </Link>
      </div>

      {/* Live Nodes Rack Overview */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between pb-3">
          <div className="flex items-center gap-2">
            <Server className="h-4 w-4 text-primary" />
            <CardTitle>{t('dashboard.nodesCard')}</CardTitle>
            <Badge variant="outline" className="font-mono text-[11px] tabular-nums">
              {nodes.filter((n) => n.status === 'ONLINE').length}/{nodes.length}
            </Badge>
          </div>
          <Button size="sm" variant="outline" asChild>
            <Link to="/nodes">
              <span>{isRu ? 'Все ноды' : 'All nodes'}</span>
              <ArrowRight className="h-3.5 w-3.5" />
            </Link>
          </Button>
        </CardHeader>
        <CardContent>
          {nodesLoading ? (
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              <Skeleton className="h-36 w-full rounded-xl" />
              <Skeleton className="h-36 w-full rounded-xl" />
              <Skeleton className="h-36 w-full rounded-xl" />
            </div>
          ) : nodes.length === 0 ? (
            <RackEmptyState
              icon={Server}
              title={t('dashboard.nodesEmpty')}
              description={
                isRu
                  ? 'Добавьте первую серверную ноду для приёма клиентского трафика.'
                  : 'Add your first proxy server node to start serving client traffic.'
              }
              action={
                <Button size="sm" asChild>
                  <Link to="/nodes?create=1">
                    <Plus className="h-3.5 w-3.5" /> {t('dashboard.addFirstNode')}
                  </Link>
                </Button>
              }
            />
          ) : (
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {nodes.slice(0, 6).map((n) => {
                const inbounds = [...(n.inbounds ?? [])].sort((a, b) => a.port - b.port);
                return (
                  <div
                    key={n.id}
                    className="node-blade relative overflow-hidden rounded-xl border border-border/80 p-3.5 pl-4 transition-colors hover:border-primary/45"
                  >
                    <div
                      className={cn(
                        'absolute inset-y-0 left-0 w-1',
                        n.status === 'ONLINE'
                          ? 'bg-emerald-400 shadow-[0_0_10px_hsl(162_84%_43%/0.6)]'
                          : n.status === 'MAINTENANCE'
                            ? 'bg-amber-400'
                            : n.status === 'ERROR'
                              ? 'bg-red-500'
                              : 'bg-zinc-600',
                      )}
                      aria-hidden="true"
                    />
                    <div className="space-y-3">
                      <div className="flex items-start justify-between gap-2">
                        <div className="min-w-0">
                          <div className="flex items-center gap-2">
                            <span
                              className={cn(
                                'h-2 w-2 shrink-0 rounded-full',
                                n.status === 'ONLINE'
                                  ? 'bg-emerald-400 animate-pulse'
                                  : n.status === 'MAINTENANCE'
                                    ? 'bg-amber-400'
                                    : 'bg-zinc-500',
                              )}
                            />
                            <Link
                              to={`/nodes/${n.id}`}
                              className="truncate text-sm font-bold hover:text-primary transition-colors"
                            >
                              {n.name}
                            </Link>
                            {n.country && (
                              <Badge variant="outline" className="px-1.5 py-0 text-[10px]">
                                {n.country}
                              </Badge>
                            )}
                          </div>
                          <div className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
                            {n.host}:{n.port}
                          </div>
                        </div>
                        <span
                          className={cn(
                            'inline-flex shrink-0 items-center gap-1 rounded border px-1.5 py-0.5 font-mono text-[10px] tabular-nums',
                            latencyBadgeClass(n.lastPingMs),
                          )}
                        >
                          <Signal className="h-2.5 w-2.5" />
                          {n.lastPingMs != null ? `${n.lastPingMs} ms` : '—'}
                        </span>
                      </div>

                      <div className="rack-inset grid grid-cols-2 gap-2.5 rounded-lg p-2.5 text-[11px]">
                        <div className="space-y-1">
                          <div className="flex justify-between font-mono tabular-nums">
                            <span className="text-muted-foreground">CPU</span>
                            <span className="font-semibold">
                              {n.cpuUsage != null ? `${n.cpuUsage.toFixed(0)}%` : '—'}
                            </span>
                          </div>
                          <MeterBar value={n.cpuUsage} tone="emerald" className="h-1" />
                        </div>
                        <div className="space-y-1">
                          <div className="flex justify-between font-mono tabular-nums">
                            <span className="text-muted-foreground">RAM</span>
                            <span className="font-semibold">
                              {n.memUsage != null ? `${n.memUsage.toFixed(0)}%` : '—'}
                            </span>
                          </div>
                          <MeterBar value={n.memUsage} tone="cyan" className="h-1" />
                        </div>
                      </div>

                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <div className="flex flex-wrap gap-1">
                          {inbounds.slice(0, 3).map((ib) => (
                            <Badge
                              key={ib.id}
                              className={cn('border px-1.5 py-0 text-[10px]', PROTOCOL_COLORS[ib.protocol])}
                            >
                              {ib.protocol}:{ib.port}
                            </Badge>
                          ))}
                          {inbounds.length > 3 && (
                            <Badge variant="outline" className="px-1.5 py-0 text-[10px]">
                              +{inbounds.length - 3}
                            </Badge>
                          )}
                          {inbounds.length === 0 && (
                            <span className="text-[11px] text-muted-foreground">
                              {isRu ? '0 протоколов' : '0 inbounds'}
                            </span>
                          )}
                        </div>
                        <Button size="sm" variant="secondary" asChild className="h-7 gap-1 px-2 text-xs">
                          <Link to={`/nodes/${n.id}`}>
                            <span>{t('common.open')}</span>
                            <ArrowRight className="h-3 w-3" />
                          </Link>
                        </Button>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Traffic Chart + Panel Host Instrument Cluster */}
      <div className="grid gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-2">
          <CardHeader className="flex flex-row flex-wrap items-center justify-between gap-2 pb-2">
            <div>
              <CardTitle>{t('dashboard.trafficHistory')}</CardTitle>
              <div className="mt-1 flex flex-wrap items-center gap-3 font-mono text-xs tabular-nums">
                <span className="text-emerald-400">
                  ↓ {formatBytes(periodTotals.dl, { locale: language })}
                </span>
                <span className="text-cyan-400">
                  ↑ {formatBytes(periodTotals.ul, { locale: language })}
                </span>
                <span className="text-muted-foreground">
                  Σ {formatBytes(periodTotals.total, { locale: language })}
                </span>
              </div>
            </div>
            <div className="flex gap-1">
              {[7, 14, 30].map((d) => (
                <Button
                  key={d}
                  size="sm"
                  variant={days === d ? 'default' : 'outline'}
                  onClick={() => setDays(d)}
                  className="h-7 px-2.5 text-xs tabular-nums"
                >
                  {d} {isRu ? 'дн' : 'd'}
                </Button>
              ))}
            </div>
          </CardHeader>
          <CardContent className="h-64">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={chartData}>
                <defs>
                  <linearGradient id="dashDlGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="#10b981" stopOpacity={0.38} />
                    <stop offset="95%" stopColor="#10b981" stopOpacity={0.02} />
                  </linearGradient>
                  <linearGradient id="dashUlGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="#06b6d4" stopOpacity={0.35} />
                    <stop offset="95%" stopColor="#06b6d4" stopOpacity={0.02} />
                  </linearGradient>
                </defs>
                <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" />
                <XAxis
                  dataKey="day"
                  tick={{ fontSize: 11, fill: 'hsl(var(--muted-foreground))' }}
                />
                <YAxis
                  width={56}
                  tickFormatter={(v) => formatBytes(v, { compact: true, locale: language })}
                  tick={{ fontSize: 11, fill: 'hsl(var(--muted-foreground))' }}
                />
                <Tooltip
                  formatter={(v: number, name: string) => [
                    formatBytes(v, { locale: language }),
                    name === 'download' ? t('dashboard.downloadLabel') : t('dashboard.uploadLabel'),
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
                  dataKey="download"
                  stackId="1"
                  stroke="#10b981"
                  strokeWidth={2}
                  fill="url(#dashDlGrad)"
                />
                <Area
                  type="monotone"
                  dataKey="upload"
                  stackId="1"
                  stroke="#06b6d4"
                  strokeWidth={2}
                  fill="url(#dashUlGrad)"
                />
              </AreaChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>

        {/* Panel Host Instrument Card */}
        <Card className="flex flex-col justify-between">
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="flex items-center gap-2">
              <Cpu className="h-4 w-4 text-primary" />
              <span>{t('dashboard.systemHealth')}</span>
            </CardTitle>
            <Badge variant="outline" className="font-mono text-[10px]">
              {hostTelemetry?.cpu.count ? `${hostTelemetry.cpu.count} vCPU` : 'HOST'}
            </Badge>
          </CardHeader>
          <CardContent className="space-y-3.5 text-xs">
            <div className="grid grid-cols-2 gap-2.5">
              <div className="rack-inset flex flex-col items-center justify-center rounded-xl p-3 text-center">
                <RadialGauge value={cpuPct} tone="emerald" size={72} label="CPU" />
                <span className="mt-1.5 text-[11px] text-muted-foreground">{t('dashboard.cpuLoad')}</span>
              </div>
              <div className="rack-inset flex flex-col items-center justify-center rounded-xl p-3 text-center">
                <RadialGauge value={memPct} tone="cyan" size={72} label="RAM" />
                <span className="mt-1.5 text-[11px] text-muted-foreground">
                  {hostTelemetry?.memory.usedBytes && hostTelemetry?.memory.totalBytes
                    ? `${formatBytes(hostTelemetry.memory.usedBytes, { compact: true, locale: language })} / ${formatBytes(hostTelemetry.memory.totalBytes, { compact: true, locale: language })}`
                    : t('dashboard.memUsage')}
                </span>
              </div>
            </div>

            <div className="grid grid-cols-2 gap-2 font-mono text-[11px] tabular-nums">
              <div className="rack-inset rounded-lg p-2.5">
                <div className="text-[10px] uppercase text-muted-foreground">Load Avg</div>
                <div className="mt-0.5 font-semibold text-foreground">
                  {hostTelemetry?.cpu.loadAverage
                    ? hostTelemetry.cpu.loadAverage.map((v) => v.toFixed(2)).join(' / ')
                    : '—'}
                </div>
              </div>
              <div className="rack-inset rounded-lg p-2.5">
                <div className="text-[10px] uppercase text-muted-foreground">
                  {isRu ? 'Аптайм' : 'Uptime'}
                </div>
                <div className="mt-0.5 font-semibold text-foreground">
                  {formatUptime(hostTelemetry?.uptime.hostSeconds, isRu)}
                </div>
              </div>
              <div className="rack-inset rounded-lg p-2.5">
                <div className="text-[10px] uppercase text-muted-foreground">TCP / UDP</div>
                <div className="mt-0.5 font-semibold text-foreground">
                  {hostTelemetry?.connections.establishedTcp ?? '—'} /{' '}
                  {hostTelemetry?.connections.udpSockets ?? '—'}
                </div>
              </div>
              <div className="rack-inset rounded-lg p-2.5">
                <div className="text-[10px] uppercase text-muted-foreground">
                  {isRu ? 'Сеть ↑/↓' : 'Net ↑/↓'}
                </div>
                <div className="mt-0.5 truncate font-semibold text-emerald-400">
                  ↓ {formatBps(hostTelemetry?.network.downloadBps, language)}
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Top Clients & Recent Activity */}
      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-3">
            <CardTitle className="flex items-center gap-2">
              <Users className="h-4 w-4 text-primary" />
              <span>{t('dashboard.topClients')}</span>
            </CardTitle>
            <Button size="sm" variant="ghost" asChild className="h-7 px-2 text-xs">
              <Link to="/clients">
                <span>{isRu ? 'Все клиенты' : 'All clients'}</span>
                <ArrowRight className="h-3.5 w-3.5" />
              </Link>
            </Button>
          </CardHeader>
          <CardContent>
            {topClients.length === 0 ? (
              <RackEmptyState
                icon={Users}
                title={t('dashboard.noTrafficYet')}
                description={
                  isRu
                    ? 'Статистика появится после подключения первых клиентов.'
                    : 'Traffic rankings will appear once clients start transferring data.'
                }
              />
            ) : (
              <div className="space-y-2">
                {topClients.map((tc, idx) => {
                  const ul = Number(tc._sum.upload || 0);
                  const dl = Number(tc._sum.download || 0);
                  const total = ul + dl;
                  const share = maxTopClientBytes > 0 ? Math.round((total / maxTopClientBytes) * 100) : 0;
                  return (
                    <Link
                      key={tc.clientId}
                      to={`/clients/${tc.clientId}/traffic`}
                      className="rack-inset block rounded-xl p-3 text-xs transition-colors hover:border-primary/45"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <div className="flex items-center gap-2 min-w-0">
                          <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-md bg-primary/15 font-mono text-[10px] font-bold text-primary">
                            #{idx + 1}
                          </span>
                          <span className="font-semibold truncate">
                            {tc.client?.username ?? tc.clientId}
                          </span>
                        </div>
                        <div className="flex items-center gap-2 font-mono tabular-nums">
                          <span className="text-[11px] text-emerald-400">
                            ↓ {formatBytes(dl, { compact: true, locale: language })}
                          </span>
                          <span className="text-[11px] text-cyan-400">
                            ↑ {formatBytes(ul, { compact: true, locale: language })}
                          </span>
                          <span className="font-bold text-foreground">
                            {formatBytes(total, { locale: language })}
                          </span>
                        </div>
                      </div>
                      <MeterBar value={share} tone="emerald" className="mt-2 h-1" />
                    </Link>
                  );
                })}
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-3">
            <CardTitle className="flex items-center gap-2">
              <ScrollText className="h-4 w-4 text-primary" />
              <span>{t('dashboard.recentAudit')}</span>
            </CardTitle>
            <Button size="sm" variant="ghost" asChild className="h-7 px-2 text-xs">
              <Link to="/audit">
                <span>{isRu ? 'Весь журнал' : 'Full log'}</span>
                <ArrowRight className="h-3.5 w-3.5" />
              </Link>
            </Button>
          </CardHeader>
          <CardContent>
            {recentAudit.length === 0 ? (
              <RackEmptyState
                icon={ScrollText}
                title={t('dashboard.noAuditYet')}
                description={
                  isRu
                    ? 'События администрирования будут отображаться здесь.'
                    : 'Administrative actions and changes will be recorded here.'
                }
              />
            ) : (
              <div className="space-y-2">
                {recentAudit.map((entry) => (
                  <div
                    key={entry.id}
                    className="rack-inset flex items-center justify-between gap-2 rounded-xl px-3 py-2.5 text-xs"
                  >
                    <div className="flex items-center gap-2 min-w-0">
                      <Badge variant={auditBadgeVariant(entry.action)} className="font-mono text-[10px]">
                        {entry.action}
                      </Badge>
                      <span className="truncate font-medium text-foreground">{entry.resource}</span>
                      {entry.user && (
                        <span className="shrink-0 font-mono text-[11px] text-primary">
                          @{entry.user.username}
                        </span>
                      )}
                    </div>
                    <span className="inline-flex shrink-0 items-center gap-1 font-mono text-[11px] text-muted-foreground tabular-nums">
                      <Clock className="h-3 w-3" />
                      {formatDateTime(entry.createdAt, timezone)}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
