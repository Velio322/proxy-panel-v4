import { useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Activity, Globe, RefreshCw, Search, Server, Trash2, Users } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Skeleton } from '@/components/ui/skeleton';
import { PageHero, RackEmptyState, StatusFilterPills } from '@/components/ui/PagePrimitives';
import { getApiErrorMessage, nodesApi, sessionsApi } from '@/lib/api';
import { PROTOCOL_COLORS, cn, formatBytes, formatDateTime } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { Protocol, SessionEntry } from '@/types';

type RefreshIntervalOpt = 'off' | '5000' | '15000';

export function SessionsExplorerPage() {
  const qc = useQueryClient();
  const timezone = useUiStore((s) => s.timezone);
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const [nodeFilter, setNodeFilter] = useState<string>('ALL');
  const [protocolFilter, setProtocolFilter] = useState<string>('ALL');
  const [search, setSearch] = useState('');
  const [autoRefresh, setAutoRefresh] = useState<RefreshIntervalOpt>('15000');

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
  });

  const {
    data: sessions = [],
    isLoading,
    refetch,
  } = useQuery({
    queryKey: ['sessions', nodeFilter, search],
    queryFn: async () => {
      if (nodeFilter !== 'ALL') {
        try {
          return await nodesApi.getSessions(nodeFilter);
        } catch {
          return sessionsApi.list({ nodeId: nodeFilter, search: search.trim() || undefined });
        }
      }
      return sessionsApi.list({
        search: search.trim() || undefined,
      });
    },
    refetchInterval: (() => {
      if (autoRefresh === 'off') return false;
      const v = Number(autoRefresh);
      return Number.isFinite(v) && v >= 5_000 ? v : false;
    })(),
  });

  const dropMut = useMutation({
    mutationFn: async (session: SessionEntry) => {
      const payload = {
        sessionId: session.id,
        nodeId: session.nodeId,
        clientId: session.clientId ?? undefined,
        ip: session.sourceIp,
      };
      if (session.nodeId) {
        try {
          await nodesApi.dropSession(session.nodeId, {
            sessionId: session.id,
            clientId: session.clientId ?? undefined,
            ip: session.sourceIp,
          });
          return;
        } catch {
          // fallback to global /sessions/drop
        }
      }
      await sessionsApi.drop(payload);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sessions'] });
      toast({ title: isRu ? 'Сессия разорвана' : 'Session dropped', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка сброса сессии' : 'Drop session failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const protocols = useMemo(() => {
    const set = new Set<string>();
    for (const s of sessions) {
      if (s.protocol) set.add(s.protocol.toUpperCase());
    }
    return Array.from(set).sort();
  }, [sessions]);

  const filteredSessions = useMemo(() => {
    const q = search.trim().toLowerCase();
    return sessions.filter((s) => {
      if (nodeFilter !== 'ALL' && s.nodeId !== nodeFilter) return false;
      if (protocolFilter !== 'ALL' && (s.protocol || '').toUpperCase() !== protocolFilter) return false;
      if (!q) return true;
      return (
        (s.username || '').toLowerCase().includes(q) ||
        (s.clientId || '').toLowerCase().includes(q) ||
        (s.sourceIp || '').toLowerCase().includes(q) ||
        (s.nodeName || '').toLowerCase().includes(q) ||
        (s.inboundTag || '').toLowerCase().includes(q) ||
        (s.target || '').toLowerCase().includes(q)
      );
    });
  }, [sessions, nodeFilter, protocolFilter, search]);

  const kpis = useMemo(() => {
    const clientsSet = new Set<string>();
    const ipSet = new Set<string>();
    let totalUl = 0;
    let totalDl = 0;

    for (const s of filteredSessions) {
      if (s.username || s.clientId) clientsSet.add(s.username || s.clientId || '');
      if (s.sourceIp) ipSet.add(s.sourceIp);
      totalUl += Number(s.uploadBytes ?? 0);
      totalDl += Number(s.downloadBytes ?? 0);
    }

    return {
      total: filteredSessions.length,
      uniqueClients: clientsSet.size,
      uniqueIps: ipSet.size,
      totalUl,
      totalDl,
    };
  }, [filteredSessions]);

  const nodePills = useMemo(
    () => [
      { key: 'ALL', label: isRu ? 'Все ноды' : 'All Nodes', count: sessions.length },
      ...nodes.map((n) => ({
        key: n.id,
        label: n.name,
        dotClass: n.status === 'ONLINE' ? 'bg-emerald-400' : 'bg-zinc-500',
      })),
    ],
    [nodes, sessions.length, isRu],
  );

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Монитор активных сессий' : 'Sessions Explorer'}
        subtitle={
          isRu
            ? 'Просмотр активных клиентских подключений, IP-адресов и принудительный сброс туннелей на нодах'
            : 'Inspect active user/IP sessions across all proxy nodes and terminate individual connections.'
        }
        icon={<Activity className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs tabular-nums">
            {filteredSessions.length} {isRu ? 'активных' : 'active'}
          </Badge>
        }
        actions={
          <>
            <Select value={autoRefresh} onValueChange={(v) => setAutoRefresh(v as RefreshIntervalOpt)}>
              <SelectTrigger className="w-40 h-8 text-xs" aria-label="Auto refresh">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="off">{isRu ? 'Автообновление: Выкл' : 'Auto-refresh: Off'}</SelectItem>
                <SelectItem value="5000">{isRu ? 'Каждые 5с' : 'Every 5s'}</SelectItem>
                <SelectItem value="15000">{isRu ? 'Каждые 15с' : 'Every 15s'}</SelectItem>
              </SelectContent>
            </Select>
            <Button variant="outline" size="sm" onClick={() => refetch()}>
              <RefreshCw className="h-4 w-4" />
              <span>{isRu ? 'Обновить' : 'Refresh'}</span>
            </Button>
          </>
        }
      />

      {/* Sessions KPI chip strip */}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded-xl border border-border/80 bg-background/60 px-3 py-2 text-[11px]">
        <span className="inline-flex items-center gap-1 font-mono tabular-nums">
          <Activity className="h-3 w-3 text-emerald-400" />
          <span className="font-bold text-foreground">{kpis.total}</span>
          <span className="text-muted-foreground">{isRu ? 'активных туннелей' : 'active connections'}</span>
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Users className="h-3 w-3 text-primary" />
          <span className="font-bold text-foreground">{kpis.uniqueClients}</span>
          {isRu ? 'клиентов' : 'clients'}
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Globe className="h-3 w-3 text-cyan-400" />
          <span className="font-bold text-foreground">{kpis.uniqueIps}</span>
          {isRu ? 'уникальных IP' : 'source IPs'}
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Server className="h-3 w-3 text-teal-400" />
          <span className="text-emerald-400">↓{formatBytes(kpis.totalDl, { compact: true, locale: language })}</span>
          <span className="text-cyan-400">↑ {formatBytes(kpis.totalUl, { compact: true, locale: language })}</span>
        </span>
      </div>


      {/* Filter Toolbar */}
      <Card>
        <CardHeader className="py-2.5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            {nodes.length <= 6 ? (
              <StatusFilterPills items={nodePills} value={nodeFilter} onChange={setNodeFilter} />
            ) : (
              <Select value={nodeFilter} onValueChange={setNodeFilter}>
                <SelectTrigger className="w-48" aria-label="Filter by node">
                  <SelectValue placeholder={isRu ? 'Все ноды' : 'All Nodes'} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="ALL">{isRu ? 'Все ноды' : 'All Nodes'}</SelectItem>
                  {nodes.map((n) => (
                    <SelectItem key={n.id} value={n.id}>
                      {n.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}

            <div className="flex flex-wrap items-center gap-2.5 flex-1 justify-end min-w-[260px]">
              <div className="relative flex-1 min-w-[220px] max-w-md">
                <Search className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
                <Input
                  placeholder={
                    isRu
                      ? 'Поиск по клиенту, IP, ноде, протоколу, цели...'
                      : 'Search client, IP, node, inbound tag, target...'
                  }
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  className="pl-9"
                />
              </div>

              <Select value={protocolFilter} onValueChange={setProtocolFilter}>
                <SelectTrigger className="w-40" aria-label="Filter by protocol">
                  <SelectValue placeholder={isRu ? 'Все протоколы' : 'All Protocols'} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="ALL">{isRu ? 'Все протоколы' : 'All Protocols'}</SelectItem>
                  {protocols.map((p) => (
                    <SelectItem key={p} value={p}>
                      {p}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
        </CardHeader>
      </Card>

      {isLoading ? (
        <Card>
          <CardContent className="space-y-2 py-4">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </CardContent>
        </Card>
      ) : filteredSessions.length === 0 ? (
        <RackEmptyState
          icon={Activity}
          title={isRu ? 'Активных сессий не обнаружено' : 'No active sessions found'}
          description={
            isRu
              ? 'Когда клиенты установят активные соединения через прокси-ноды, их сессии появятся здесь.'
              : 'Active client connections across proxy nodes will appear here in real time.'
          }
        />
      ) : (
        <Card>
          <CardContent className="p-0">
            <div className="overflow-x-auto">
              <table className="data-table w-full text-xs">
                <thead className="bg-muted/40">
                  <tr className="border-b border-border/80 text-left text-muted-foreground">
                    <th className="py-1.5 px-2">{isRu ? 'Клиент' : 'Client'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Нода' : 'Node'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Протокол / Тег' : 'Protocol / Inbound'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'IP клиента' : 'Source IP'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Назначение' : 'Target'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Трафик' : 'Traffic'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Подключён' : 'Connected At'}</th>
                    <th className="py-1.5 px-2 text-right">{isRu ? 'Действие' : 'Action'}</th>
                  </tr>
                </thead>
                <tbody>
                  {filteredSessions.map((s) => {
                    const resolvedNodeName =
                      s.nodeName || nodes.find((n) => n.id === s.nodeId)?.name || s.nodeId;
                    const upProto = (s.protocol || '').toUpperCase() as Protocol;
                    const protoColor = PROTOCOL_COLORS[upProto];
                    return (
                      <tr key={s.id} className="border-b border-border/60 last:border-b-0 hover:bg-accent/35">
                        <td className="py-1.5 px-2 font-semibold">
                          {s.username || s.clientId || 'anonymous'}
                        </td>
                        <td className="py-1.5 px-2">
                          <span className="inline-flex items-center gap-1.5">
                            <Server className="h-3.5 w-3.5 text-primary" />
                            <span>{resolvedNodeName}</span>
                          </span>
                        </td>
                        <td className="py-1.5 px-2">
                          <div className="flex items-center gap-1.5">
                            {s.protocol && (
                              <Badge
                                variant="outline"
                                className={cn('text-[10px]', protoColor && `border ${protoColor}`)}
                              >
                                {s.protocol}
                              </Badge>
                            )}
                            {s.inboundTag && (
                              <span className="font-mono text-muted-foreground">{s.inboundTag}</span>
                            )}
                          </div>
                        </td>
                        <td className="py-1.5 px-2 font-mono">{s.sourceIp}</td>
                        <td className="py-1.5 px-2 font-mono text-muted-foreground">
                          {s.target || '—'}
                        </td>
                        <td className="py-1.5 px-2 font-mono tabular-nums">
                          <span className="text-emerald-400">
                            ↓ {formatBytes(s.downloadBytes ?? 0, { compact: true, locale: language })}
                          </span>{' '}
                          /{' '}
                          <span className="text-cyan-400">
                            ↑ {formatBytes(s.uploadBytes ?? 0, { compact: true, locale: language })}
                          </span>
                        </td>
                        <td className="py-1.5 px-2 font-mono text-muted-foreground">
                          {formatDateTime(s.connectedAt, timezone)}
                        </td>
                        <td className="py-1.5 px-2 text-right">
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            className="h-7 text-xs"
                            onClick={() => dropMut.mutate(s)}
                            loading={dropMut.isPending}
                            data-testid={`drop-session-${s.id}`}
                          >
                            <Trash2 className="h-3.5 w-3.5 text-red-400" />
                            <span>{isRu ? 'Сбросить' : 'Drop Session'}</span>
                          </Button>
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
    </div>
  );
}
