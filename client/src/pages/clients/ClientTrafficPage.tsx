import { useMemo, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
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
import { Activity, ArrowLeft, Calendar, Server, Smartphone } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { PageHero, RackEmptyState } from '@/components/ui/PagePrimitives';
import { MeterBar } from '@/components/nodes/NodeTelemetryPrimitives';
import { clientsApi } from '@/lib/api';
import { PROTOCOL_COLORS, cn, formatBytes, formatDate, formatDateTime } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';

export function ClientTrafficPage() {
  const { id = '' } = useParams<{ id: string }>();
  const [days, setDays] = useState(30);
  const timezone = useUiStore((s) => s.timezone);
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const { data: client } = useQuery({
    queryKey: ['client', id],
    queryFn: () => clientsApi.get(id),
    enabled: Boolean(id),
  });

  const { data: daily = [] } = useQuery({
    queryKey: ['client-traffic-daily', id, days],
    queryFn: () => clientsApi.getTrafficDaily(id, days),
    enabled: Boolean(id),
  });

  const { data: nodeStats = [] } = useQuery({
    queryKey: ['client-node-stats', id, days],
    queryFn: () => clientsApi.getStats(id, days),
    enabled: Boolean(id),
  });

  const { data: devices = [] } = useQuery({
    queryKey: ['client-devices', id],
    queryFn: () => clientsApi.listDevices(id),
    enabled: Boolean(id),
  });

  const chartData = daily.map((row) => ({
    day: formatDate(row.day, { month: 'short', day: 'numeric' }, timezone),
    upload: Number(row.upload || 0),
    download: Number(row.download || 0),
  }));

  const used = Number(client?.usedTraffic || 0);
  const limit = Number(client?.trafficLimit || 0);
  const pct = limit > 0 ? Math.min(100, Math.round((used / limit) * 100)) : 0;

  const maxNodeTraffic = useMemo(() => {
    let max = 0;
    for (const ns of nodeStats) {
      const sum = Number(ns.upload || 0) + Number(ns.download || 0);
      if (sum > max) max = sum;
    }
    return max;
  }, [nodeStats]);

  return (
    <div className="space-y-5">
      <PageHero
        title={
          isRu
            ? `Телеметрия клиента: ${client?.username ?? id}`
            : `Client Traffic: ${client?.username ?? id}`
        }
        subtitle={
          isRu
            ? 'Посуточная динамика трафика, распределение нагрузки по нодам и зарегистрированные HWID-устройства'
            : 'Daily bandwidth consumption, per-node traffic distribution, and registered HWID devices'
        }
        icon={<Activity className="h-4 w-4" />}
        badge={
          client ? (
            <Badge variant={client.banned ? 'destructive' : 'success'}>
              {client.banned ? (isRu ? 'Заблокирован' : 'Banned') : isRu ? 'Активен' : 'Active'}
            </Badge>
          ) : undefined
        }
        actions={
          <>
            <Button variant="outline" size="sm" asChild>
              <Link to="/clients">
                <ArrowLeft className="h-4 w-4" />
                <span>{isRu ? 'К списку клиентов' : 'Back to Clients'}</span>
              </Link>
            </Button>
            <div className="flex gap-1">
              {[7, 30, 90].map((d) => (
                <Button
                  key={d}
                  size="sm"
                  variant={days === d ? 'default' : 'outline'}
                  onClick={() => setDays(d)}
                  className="h-8 px-2.5 text-xs tabular-nums"
                >
                  {d} {isRu ? 'дн' : 'd'}
                </Button>
              ))}
            </div>
          </>
        }
        meta={
          client && (client.protocols?.length ?? 0) > 0 ? (
            <div className="flex flex-wrap items-center gap-1.5">
              {client.protocols.map((p) => (
                <Badge key={p} className={cn('border px-1.5 py-0 text-[10px]', PROTOCOL_COLORS[p])}>
                  {p}
                </Badge>
              ))}
            </div>
          ) : undefined
        }
      />

      {/* 4-Card Summary Ribbon */}
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Card className="p-3 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Использовано трафика' : 'Used Traffic'}</span>
            <Activity className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {formatBytes(used, { locale: language })}
          </div>
          <MeterBar value={limit > 0 ? pct : 35} tone="emerald" />
        </Card>

        <Card className="p-3 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Лимит и сброс' : 'Quota & Reset'}</span>
            <Calendar className="h-4 w-4 text-cyan-400" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {limit > 0 ? formatBytes(limit, { locale: language }) : isRu ? 'Безлимит' : 'Unlimited'}
          </div>
          <div className="text-[11px] text-muted-foreground">
            {client?.nextTrafficResetAt
              ? `${isRu ? 'Сброс:' : 'Next reset:'} ${formatDate(client.nextTrafficResetAt, undefined, timezone)}`
              : isRu
                ? `Истекает: ${client?.expireAt ? formatDate(client.expireAt, undefined, timezone) : 'Никогда'}`
                : `Expires: ${client?.expireAt ? formatDate(client.expireAt, undefined, timezone) : 'Never'}`}
          </div>
        </Card>

        <Card className="p-3 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Задействовано нод' : 'Active Nodes'}</span>
            <Server className="h-4 w-4 text-primary" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">{nodeStats.length}</div>
          <div className="text-[11px] text-muted-foreground">
            {client?.allNodes !== false
              ? isRu
                ? 'Доступ ко всем нодам'
                : 'All nodes allowed'
              : isRu
                ? `Привязано нод: ${client?.nodes?.length ?? 0}`
                : `Pinned nodes: ${client?.nodes?.length ?? 0}`}
          </div>
        </Card>

        <Card className="p-3 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'HWID Устройства' : 'HWID Devices'}</span>
            <Smartphone className="h-4 w-4 text-amber-400" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {devices.length} / {client?.deviceLimit || '∞'}
          </div>
          <div className="text-[11px] text-muted-foreground">
            {isRu ? 'Зарегистрированные клиенты' : 'Registered client endpoints'}
          </div>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>
            {isRu ? `Посуточный трафик (${days} дн.)` : `Daily Traffic (${days} days)`}
          </CardTitle>
        </CardHeader>
        <CardContent className="h-72">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={chartData}>
              <defs>
                <linearGradient id="clientDlGrad" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#10b981" stopOpacity={0.38} />
                  <stop offset="95%" stopColor="#10b981" stopOpacity={0.02} />
                </linearGradient>
                <linearGradient id="clientUlGrad" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#06b6d4" stopOpacity={0.35} />
                  <stop offset="95%" stopColor="#06b6d4" stopOpacity={0.02} />
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" />
              <XAxis dataKey="day" tick={{ fontSize: 11, fill: 'hsl(var(--muted-foreground))' }} />
              <YAxis
                tickFormatter={(v) => formatBytes(v, { compact: true, locale: language })}
                tick={{ fontSize: 11, fill: 'hsl(var(--muted-foreground))' }}
              />
              <Tooltip
                formatter={(v: number, name: string) => [
                  formatBytes(v, { locale: language }),
                  name === 'download'
                    ? isRu
                      ? 'Загрузка ↓'
                      : 'Download ↓'
                    : isRu
                      ? 'Отдача ↑'
                      : 'Upload ↑',
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
                fill="url(#clientDlGrad)"
              />
              <Area
                type="monotone"
                dataKey="upload"
                stackId="1"
                stroke="#06b6d4"
                strokeWidth={2}
                fill="url(#clientUlGrad)"
              />
            </AreaChart>
          </ResponsiveContainer>
        </CardContent>
      </Card>

      <div className="grid gap-3 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>{isRu ? 'Распределение по нодам' : 'Per-Node Breakdown'}</CardTitle>
          </CardHeader>
          <CardContent>
            {nodeStats.length === 0 ? (
              <RackEmptyState
                icon={Server}
                title={isRu ? 'Трафик по нодам пока не записан' : 'No per-node traffic recorded yet'}
                description={
                  isRu
                    ? 'Данные появятся после передачи трафика через прокси-ноды.'
                    : 'Node breakdown will populate once traffic flows through proxy nodes.'
                }
              />
            ) : (
              <div className="space-y-2.5">
                {nodeStats.map((ns) => {
                  const total = Number(ns.upload || 0) + Number(ns.download || 0);
                  const share = maxNodeTraffic > 0 ? Math.round((total / maxNodeTraffic) * 100) : 0;
                  return (
                    <Link
                      key={ns.nodeId}
                      to={`/nodes/${ns.nodeId}`}
                      className="rack-inset block rounded-xl p-3 text-xs transition-colors hover:border-primary/45"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-semibold">{ns.nodeName || ns.nodeId}</span>
                        <span className="font-mono tabular-nums">
                          <span className="text-emerald-400">
                            ↓ {formatBytes(ns.download, { locale: language })}
                          </span>{' '}
                          /{' '}
                          <span className="text-cyan-400">
                            ↑ {formatBytes(ns.upload, { locale: language })}
                          </span>
                        </span>
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
          <CardHeader>
            <CardTitle>
              {isRu
                ? `Зарегистрированные HWID-устройства (${devices.length})`
                : `Registered HWID Devices (${devices.length})`}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {devices.length === 0 ? (
              <RackEmptyState
                icon={Smartphone}
                title={isRu ? 'Нет зарегистрированных устройств' : 'No registered devices'}
                description={
                  isRu
                    ? 'Устройства регистрируются автоматически при запросе подписки с заголовком X-HWID.'
                    : 'Devices register automatically when fetching subscriptions with an X-HWID header.'
                }
              />
            ) : (
              <div className="space-y-2">
                {devices.map((dev) => (
                  <div
                    key={dev.id}
                    className="rack-inset flex items-center justify-between rounded-xl px-3 py-2.5 text-xs"
                  >
                    <div className="flex items-center gap-2 min-w-0">
                      <Smartphone className="h-3.5 w-3.5 text-primary shrink-0" />
                      <span className="font-mono break-words min-w-0">{dev.hwid}</span>
                      {dev.os && <Badge variant="outline">{dev.os}</Badge>}
                    </div>
                    <span className="font-mono text-[11px] text-muted-foreground shrink-0">
                      {formatDateTime(dev.lastSeenAt, timezone)}
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
