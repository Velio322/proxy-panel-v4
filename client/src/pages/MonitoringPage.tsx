import { useEffect, useMemo, useState } from 'react';
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
  Cpu,
  HardDrive,
  Network,
  Server,
  Signal,
  Terminal,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { PageHero, RackEmptyState } from '@/components/ui/PagePrimitives';
import { MeterBar, formatUptime, latencyBadgeClass } from '@/components/nodes/NodeTelemetryPrimitives';
import { monitoringApi, nodesApi } from '@/lib/api';
import { cn, formatBps, formatBytes } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';

export function MonitoringPage() {
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';
  const [selectedNodeId, setSelectedNodeId] = useState<string>('');
  const [nodeHistory, setNodeHistory] = useState<
    Array<{ time: string; cpu: number; mem: number }>
  >([]);

  const { data: host } = useQuery({
    queryKey: ['monitoring-host'],
    queryFn: () => monitoringApi.host(),
    refetchInterval: 5_000,
  });

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
    refetchInterval: 15_000,
  });

  useEffect(() => {
    if (nodes.length === 0) {
      if (selectedNodeId) setSelectedNodeId('');
      return;
    }
    if (!selectedNodeId || !nodes.some((n) => n.id === selectedNodeId)) {
      setSelectedNodeId(nodes[0].id);
    }
  }, [nodes, selectedNodeId]);

  useEffect(() => {
    setNodeHistory([]);
  }, [selectedNodeId]);

  const { data: nodeTelemetry } = useQuery({
    queryKey: ['monitoring-node', selectedNodeId],
    queryFn: () => monitoringApi.node(selectedNodeId),
    enabled: Boolean(selectedNodeId),
    refetchInterval: 5_000,
  });

  const m = nodeTelemetry?.metrics;
  const activeNode = nodes.find((n) => n.id === selectedNodeId);

  useEffect(() => {
    if (!m) return;
    const cpu = m.cpuUsage != null && Number.isFinite(m.cpuUsage) ? Math.max(0, Math.min(100, m.cpuUsage)) : null;
    const mem = m.memUsage != null && Number.isFinite(m.memUsage) ? Math.max(0, Math.min(100, m.memUsage)) : null;
    if (cpu === null && mem === null) return;
    const nowLabel = new Date().toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
    setNodeHistory((prev) => [
      ...prev.slice(-23),
      { time: nowLabel, cpu: Number((cpu ?? 0).toFixed(1)), mem: Number((mem ?? 0).toFixed(1)) },
    ]);
  }, [m?.sampledAt, m?.cpuUsage, m?.memUsage]);

  const hostCpu =
    host && host.cpu.usagePercent >= 0 ? Math.max(0, Math.min(100, host.cpu.usagePercent)) : null;
  const nodeCpu =
    m?.cpuUsage != null && Number.isFinite(m.cpuUsage)
      ? Math.max(0, Math.min(100, m.cpuUsage))
      : activeNode?.cpuUsage ?? null;
  const nodeMem =
    m?.memUsage != null && Number.isFinite(m.memUsage)
      ? Math.max(0, Math.min(100, m.memUsage))
      : activeNode?.memUsage ?? null;

  const coreEngineCards = useMemo(() => {
    type CoreKey = 'xray' | 'singbox' | 'naive' | 'mieru' | 'telemt';
    const resolveRunning = (key: CoreKey): boolean => {
      if (!m) return false;
      const boolField = m[`${key}Running` as keyof typeof m];
      if (typeof boolField === 'boolean') return boolField;
      const coreObj = m.cores?.[key];
      if (coreObj && typeof coreObj.running === 'boolean') return coreObj.running;
      const st = m.coreStatuses?.[key];
      if (typeof st === 'string') return st.toLowerCase() === 'running';
      return false;
    };

    const resolveVersion = (key: CoreKey): string | null => {
      const fromMetrics = m?.cores?.[key]?.version;
      if (fromMetrics) return fromMetrics;
      if (!activeNode) return null;
      if (key === 'xray') return activeNode.xrayVersion ?? null;
      if (key === 'singbox') return activeNode.singboxVersion ?? null;
      if (key === 'naive') return activeNode.naiveVersion ?? null;
      if (key === 'mieru') return activeNode.mieruVersion ?? null;
      return activeNode.telemtVersion ?? null;
    };

    return [
      {
        key: 'xray' as const,
        label: 'Xray-core',
        running: resolveRunning('xray'),
        pid: m?.xrayPid ?? null,
        version: resolveVersion('xray'),
      },
      {
        key: 'singbox' as const,
        label: 'Sing-box',
        running: resolveRunning('singbox'),
        pid: m?.singboxPid ?? null,
        version: resolveVersion('singbox'),
      },
      {
        key: 'naive' as const,
        label: 'NaiveProxy',
        running: resolveRunning('naive'),
        pid: m?.naivePid ?? null,
        version: resolveVersion('naive'),
      },
      {
        key: 'mieru' as const,
        label: 'Mieru (mita)',
        running: resolveRunning('mieru'),
        pid: m?.mieruPid ?? null,
        version: resolveVersion('mieru'),
      },
      {
        key: 'telemt' as const,
        label: 'telemt (TGWEB)',
        running: resolveRunning('telemt'),
        pid: null,
        version: resolveVersion('telemt'),
      },
    ];
  }, [m, activeNode]);

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Мониторинг системы и нод' : 'System & Node Monitoring'}
        subtitle={
          isRu
            ? 'Телеметрия хоста панели в реальном времени, нагрузка ядер на нодах, сокеты и активные туннели'
            : 'Real-time panel host telemetry, per-node socket metrics, core engine status, and active online users'
        }
        icon={<Cpu className="h-4 w-4" />}
        badge={
          <Badge variant="success" className="gap-1.5 font-mono text-xs">
            <span className="h-2 w-2 animate-pulse rounded-full bg-emerald-400" />
            {isRu ? 'LIVE 5с' : 'LIVE 5s'}
          </Badge>
        }
        actions={
          <>
            <Button variant="outline" size="sm" asChild>
              <Link to="/sessions">
                <Activity className="h-4 w-4" />
                <span>{isRu ? 'Активные сессии' : 'Sessions Explorer'}</span>
              </Link>
            </Button>
            <Button variant="outline" size="sm" asChild>
              <Link to="/logs">
                <Terminal className="h-4 w-4" />
                <span>{isRu ? 'Логи нод' : 'Live Logs'}</span>
              </Link>
            </Button>
            <Button variant="outline" size="sm" asChild>
              <Link to="/nodes">
                <Server className="h-4 w-4" />
                <span>{isRu ? 'Все ноды' : 'Nodes'}</span>
              </Link>
            </Button>
          </>
        }
      />

      {/* Panel host vitals chip strip */}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded-xl border border-border/80 bg-background/60 px-3 py-2 text-[11px]">
        <span className="inline-flex items-center gap-1 font-mono tabular-nums">
          <Cpu className="h-3 w-3 text-emerald-400" />
          <span className="font-bold text-foreground">{hostCpu !== null ? `${hostCpu.toFixed(0)}%` : '—'}</span>
          CPU
          <span className="text-muted-foreground">· {host?.cpu.count ? `${host.cpu.count} vCPU` : 'Control Plane'}</span>
          <span>load {host?.cpu.loadAverage ? host.cpu.loadAverage.map((v) => v.toFixed(2)).join('/') : '—'}</span>
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <HardDrive className="h-3 w-3 text-cyan-400" />
          <span className="font-semibold text-foreground">
            {host?.memory.usedBytes && host?.memory.totalBytes
              ? `${formatBytes(host.memory.usedBytes, { compact: true, locale: language })} / ${formatBytes(host.memory.totalBytes, { compact: true, locale: language })}`
              : host?.process?.memory?.rss
                ? `RSS ${formatBytes(host.process.memory.rss, { compact: true, locale: language })}`
                : '—'}
          </span>
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Network className="h-3 w-3 text-primary" />
          <span className="text-cyan-400 font-semibold">↓ {formatBps(host?.network.downloadBps, language)}</span>
          <span className="text-emerald-400 font-semibold">↑ {formatBps(host?.network.uploadBps, language)}</span>
          <span>RX {formatBytes(host?.network.receivedBytes ?? 0, { compact: true, locale: language })} · TX {formatBytes(host?.network.transmittedBytes ?? 0, { compact: true, locale: language })}</span>
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Activity className="h-3 w-3 text-teal-400" />
          <span className="font-bold text-foreground">
            {host && host.connections.establishedTcp >= 0 ? `${host.connections.establishedTcp} TCP / ${host.connections.udpSockets} UDP` : '—'}
          </span>
          <span>{isRu ? 'хост' : 'host'} {formatUptime(host?.uptime.hostSeconds, isRu)}</span>
          <span>· {isRu ? 'панель' : 'panel'} {formatUptime(host?.uptime.processSeconds, isRu)}</span>
        </span>
      </div>

      {/* Per-Node Live Telemetry Console */}
      <Card>
        <CardHeader className="space-y-3 pb-2">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <CardTitle className="flex items-center gap-2">
              <Server className="h-4 w-4 text-primary" />
              <span>{isRu ? 'Телеметрия прокси-ноды' : 'Node Telemetry Matrix'}</span>
            </CardTitle>
            {activeNode && (
              <Button size="sm" variant="secondary" asChild className="h-8 gap-1.5 text-xs">
                <Link to={`/nodes/${activeNode.id}`}>
                  <span>{isRu ? `Консоль ${activeNode.name}` : `Open ${activeNode.name}`}</span>
                  <ArrowRight className="h-3.5 w-3.5" />
                </Link>
              </Button>
            )}
          </div>

          {/* Interactive Node Pill Switcher */}
          {nodes.length > 0 && (
            <div className="flex flex-wrap items-center gap-2">
              {nodes.map((n) => {
                const selected = n.id === selectedNodeId;
                return (
                  <button
                    key={n.id}
                    type="button"
                    onClick={() => setSelectedNodeId(n.id)}
                    className={cn(
                      'inline-flex items-center gap-2 rounded-xl border px-3 py-1.5 text-xs font-medium transition-all',
                      selected
                        ? 'border-primary/55 bg-primary/15 text-foreground shadow-sm'
                        : 'border-border/75 bg-background/45 text-muted-foreground hover:border-border hover:text-foreground',
                    )}
                  >
                    <span
                      className={cn(
                        'h-2 w-2 rounded-full',
                        n.status === 'ONLINE'
                          ? 'bg-emerald-400 animate-pulse'
                          : n.status === 'MAINTENANCE'
                            ? 'bg-amber-400'
                            : 'bg-zinc-500',
                      )}
                    />
                    <span className="font-semibold">{n.name}</span>
                    <span className="font-mono text-[11px] opacity-75">{n.host}</span>
                    {n.lastPingMs != null && (
                      <span className="font-mono text-[10px] text-emerald-400">{n.lastPingMs}ms</span>
                    )}
                  </button>
                );
              })}
            </div>
          )}
        </CardHeader>

        <CardContent>
          {nodes.length === 0 ? (
            <RackEmptyState
              icon={Server}
              title={isRu ? 'Нет подключённых нод' : 'No proxy nodes available'}
              description={
                isRu
                  ? 'Добавьте серверную ноду для сбора метрик CPU, RAM и состояния прокси-ядер.'
                  : 'Add a proxy node to inspect real-time CPU, RAM, and core engine metrics.'
              }
            />
          ) : !m && !activeNode ? (
            <RackEmptyState
              icon={Activity}
              title={isRu ? 'Выберите ноду для мониторинга' : 'Select a node to inspect telemetry'}
            />
          ) : (
            <div className="space-y-4">
              {/* Node vitals chip strip */}
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded-xl border border-border/80 bg-background/60 px-3 py-2 text-[11px]">
                <span className="inline-flex items-center gap-1 font-mono tabular-nums">
                  <Cpu className="h-3 w-3 text-emerald-400" />
                  <span className="font-bold text-foreground">{nodeCpu !== null ? `${nodeCpu.toFixed(0)}%` : '—'}</span>
                  CPU
                  <span className="text-muted-foreground">· {activeNode?.arch || m?.cores?.arch || 'x86_64'}</span>
                </span>
                <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
                  <HardDrive className="h-3 w-3 text-cyan-400" />
                  <span className="font-bold text-foreground">{nodeMem !== null ? `${nodeMem.toFixed(0)}%` : '—'}</span>
                  {isRu ? 'память' : 'mem'}
                  <span>· {isRu ? 'агент' : 'agent'} v{m?.version || activeNode?.version || '4.0'}</span>
                </span>
                <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
                  <Network className="h-3 w-3 text-primary" />
                  <span className="text-cyan-400 font-semibold">↓ {formatBps(m?.downloadBps, language)}</span>
                  <span className="text-emerald-400 font-semibold">↑ {formatBps(m?.uploadBps, language)}</span>
                  <span>Σ ↓ {formatBytes(activeNode?.netDownload ?? 0, { compact: true, locale: language })}</span>
                  <span>· ↑ {formatBytes(activeNode?.netUpload ?? 0, { compact: true, locale: language })}</span>
                </span>
                <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
                  <Activity className="h-3 w-3 text-teal-400" />
                  <span className="font-bold text-foreground">{m?.onlineUsersCount !== undefined ? m.onlineUsersCount : 0}</span>
                  {isRu ? 'онлайн' : 'online'}
                  <span>· TCP {m?.tcpConnections ?? '—'}</span>
                  <span>· UDP {m?.udpSockets ?? '—'}</span>
                  <span>· {formatUptime(m?.uptime, isRu)}</span>
                </span>
              </div>

              {/* 4-Core Engine Status Matrix */}
              <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
                {coreEngineCards.map((c) => (
                  <div
                    key={c.key}
                    className="rack-inset flex items-center justify-between rounded-xl p-2 text-xs"
                  >
                    <div className="space-y-1">
                      <div className="flex items-center gap-2">
                        <span
                          className={cn(
                            'h-2 w-2 rounded-full',
                            c.running
                              ? 'bg-emerald-400 shadow-[0_0_8px_hsl(162_84%_43%/0.8)]'
                              : 'bg-zinc-500',
                          )}
                        />
                        <span className="font-bold">{c.label}</span>
                      </div>
                      <div className="font-mono text-[11px] text-muted-foreground">
                        {c.version ? `v${c.version.replace(/^v/i, '')}` : isRu ? 'версия —' : 'ver —'}
                        {c.pid ? ` · PID ${c.pid}` : ''}
                      </div>
                    </div>
                    <Badge variant={c.running ? 'success' : 'secondary'} className="text-[10px]">
                      {c.running
                        ? isRu
                          ? 'Запущено'
                          : 'Running'
                        : isRu
                          ? 'Остановлено'
                          : 'Stopped'}
                    </Badge>
                  </div>
                ))}
              </div>

              {/* Live Rolling CPU/RAM Area Chart */}
              {nodeHistory.length > 1 && (
                <div className="rack-inset rounded-xl p-2 space-y-2">
                  <div className="flex items-center justify-between text-xs">
                    <span className="font-semibold">
                      {isRu ? 'Динамика нагрузки CPU / RAM в реальном времени' : 'Live Rolling CPU / RAM Load'}
                    </span>
                    <span className="font-mono text-[11px] text-muted-foreground">
                      {nodeHistory.length} {isRu ? 'точек (шаг 5с)' : 'samples (5s interval)'}
                    </span>
                  </div>
                  <div className="h-44">
                    <ResponsiveContainer width="100%" height="100%">
                      <AreaChart data={nodeHistory}>
                        <defs>
                          <linearGradient id="monCpuGrad" x1="0" y1="0" x2="0" y2="1">
                            <stop offset="5%" stopColor="#10b981" stopOpacity={0.35} />
                            <stop offset="95%" stopColor="#10b981" stopOpacity={0.02} />
                          </linearGradient>
                          <linearGradient id="monMemGrad" x1="0" y1="0" x2="0" y2="1">
                            <stop offset="5%" stopColor="#06b6d4" stopOpacity={0.32} />
                            <stop offset="95%" stopColor="#06b6d4" stopOpacity={0.02} />
                          </linearGradient>
                        </defs>
                        <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" />
                        <XAxis
                          dataKey="time"
                          tick={{ fontSize: 10, fill: 'hsl(var(--muted-foreground))' }}
                        />
                        <YAxis
                          domain={[0, 100]}
                          width={36}
                          tickFormatter={(v) => `${v}%`}
                          tick={{ fontSize: 10, fill: 'hsl(var(--muted-foreground))' }}
                        />
                        <Tooltip
                          formatter={(v: number, name: string) => [
                            `${v}%`,
                            name === 'cpu' ? 'CPU' : 'RAM',
                          ]}
                          contentStyle={{
                            backgroundColor: 'hsl(var(--popover))',
                            borderColor: 'hsl(var(--border))',
                            borderRadius: '10px',
                            fontSize: '12px',
                          }}
                        />
                        <Area
                          type="monotone"
                          dataKey="cpu"
                          stroke="#10b981"
                          strokeWidth={2}
                          fill="url(#monCpuGrad)"
                        />
                        <Area
                          type="monotone"
                          dataKey="mem"
                          stroke="#06b6d4"
                          strokeWidth={2}
                          fill="url(#monMemGrad)"
                        />
                      </AreaChart>
                    </ResponsiveContainer>
                  </div>
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      {/* All-Nodes Fleet Comparison Grid */}
      {nodes.length > 0 && (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="flex items-center gap-2 text-sm">
              <HardDrive className="h-4 w-4 text-primary" />
              <span>{isRu ? 'Сравнение нагрузки всех нод' : 'Fleet Resource Comparison'}</span>
            </CardTitle>
          </CardHeader>
          <CardContent className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
            {nodes.map((n) => (
              <Link
                key={n.id}
                to={`/nodes/${n.id}`}
                className="rack-inset block rounded-xl p-2 text-xs transition-colors hover:border-primary/45 space-y-2"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2 min-w-0">
                    <span
                      className={cn(
                        'h-2 w-2 shrink-0 rounded-full',
                        n.status === 'ONLINE' ? 'bg-emerald-400' : 'bg-zinc-500',
                      )}
                    />
                    <span className="font-bold break-words min-w-0">{n.name}</span>
                  </div>
                  <span
                    className={cn(
                      'inline-flex items-center gap-1 rounded border px-1.5 py-0 text-[10px] font-mono tabular-nums',
                      latencyBadgeClass(n.lastPingMs),
                    )}
                  >
                    <Signal className="h-2.5 w-2.5" />
                    {n.lastPingMs != null ? `${n.lastPingMs} ms` : '—'}
                  </span>
                </div>
                <div className="space-y-1 font-mono text-[11px] tabular-nums">
                  <div className="flex items-center gap-2">
                    <span className="w-8 text-muted-foreground">CPU</span>
                    <MeterBar value={n.cpuUsage} tone="emerald" className="h-1 flex-1" />
                    <span className="w-10 text-right">
                      {n.cpuUsage != null ? `${n.cpuUsage.toFixed(0)}%` : '—'}
                    </span>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="w-8 text-muted-foreground">RAM</span>
                    <MeterBar value={n.memUsage} tone="cyan" className="h-1 flex-1" />
                    <span className="w-10 text-right">
                      {n.memUsage != null ? `${n.memUsage.toFixed(0)}%` : '—'}
                    </span>
                  </div>
                </div>
              </Link>
            ))}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
