import { useEffect, useMemo, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  AlertTriangle,
  FileText,
  RefreshCw,
  Search,
  ShieldAlert,
  Smartphone,
  Trash2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Skeleton } from '@/components/ui/skeleton';
import { PageHero, RackEmptyState, SplitBar } from '@/components/ui/PagePrimitives';
import { clientsApi, getApiErrorMessage, inspectorsApi } from '@/lib/api';
import { cn, formatDateTime } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';

type InspectorTab = 'srh' | 'hwid' | 'torrent';

function resolveInitialTab(pathname: string): InspectorTab {
  if (pathname.includes('hwid')) return 'hwid';
  if (pathname.includes('torrent')) return 'torrent';
  return 'srh';
}

export function InspectorsPage() {
  const location = useLocation();
  const qc = useQueryClient();
  const timezone = useUiStore((s) => s.timezone);
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const [tab, setTab] = useState<InspectorTab>(() => resolveInitialTab(location.pathname));
  const [srhSearch, setSrhSearch] = useState('');
  const [hwidSearch, setHwidSearch] = useState('');
  const [torrentSearch, setTorrentSearch] = useState('');

  useEffect(() => {
    setTab(resolveInitialTab(location.pathname));
  }, [location.pathname]);

  const {
    data: srhData,
    isLoading: srhLoading,
    refetch: refetchSrh,
  } = useQuery({
    queryKey: ['inspectors-srh', srhSearch],
    queryFn: () => inspectorsApi.listSrh({ search: srhSearch.trim() || undefined, limit: 100 }),
  });

  const {
    data: hwidData,
    isLoading: hwidLoading,
    refetch: refetchHwid,
  } = useQuery({
    queryKey: ['inspectors-hwid', hwidSearch],
    queryFn: () => inspectorsApi.listHwid({ search: hwidSearch.trim() || undefined, limit: 100 }),
  });

  const {
    data: torrentData,
    isLoading: torrentLoading,
    refetch: refetchTorrent,
  } = useQuery({
    queryKey: ['inspectors-torrent'],
    queryFn: () => inspectorsApi.listTorrentBlocker({ limit: 100 }),
  });

  const revokeDeviceMut = useMutation({
    mutationFn: ({ clientId, deviceId }: { clientId: string; deviceId: string }) =>
      clientsApi.deleteDevice(clientId, deviceId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['inspectors-hwid'] });
      toast({ title: isRu ? 'HWID-устройство отозвано' : 'HWID device revoked', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка отзыва' : 'Revoke failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const srhEntries = srhData?.data ?? [];
  const hwidEntries = hwidData?.data ?? [];
  const torrentEntries = torrentData?.data ?? [];

  const srhOkPct = useMemo(() => {
    if (srhEntries.length === 0) return 100;
    const ok = srhEntries.filter((x) => x.status >= 200 && x.status < 300).length;
    return Math.round((ok / srhEntries.length) * 100);
  }, [srhEntries]);

  const hwidCollisionSet = useMemo(() => {
    const map = new Map<string, Set<string>>();
    for (const d of hwidEntries) {
      if (!d.hwid || !d.clientId) continue;
      if (!map.has(d.hwid)) map.set(d.hwid, new Set());
      map.get(d.hwid)!.add(d.clientId);
    }
    const collisions = new Set<string>();
    for (const [hwid, clientSet] of map.entries()) {
      if (clientSet.size > 1) collisions.add(hwid);
    }
    return collisions;
  }, [hwidEntries]);

  const filteredTorrentEntries = useMemo(() => {
    const q = torrentSearch.trim().toLowerCase();
    if (!q) return torrentEntries;
    return torrentEntries.filter(
      (r) =>
        (r.username || '').toLowerCase().includes(q) ||
        (r.nodeName || '').toLowerCase().includes(q) ||
        (r.sourceIp || '').toLowerCase().includes(q) ||
        (r.protocol || '').toLowerCase().includes(q) ||
        (r.rule || '').toLowerCase().includes(q),
    );
  }, [torrentEntries, torrentSearch]);

  const handleRefresh = () => {
    if (tab === 'srh') refetchSrh();
    else if (tab === 'hwid') refetchHwid();
    else refetchTorrent();
  };

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Инспекторы безопасности и трафика' : 'Security & Traffic Inspectors'}
        subtitle={
          isRu
            ? 'История запросов подписок (SRH), глобальный реестр HWID-устройств и отчёты блокировщика BitTorrent'
            : 'Subscription Request History (SRH), global HWID device registry, and Torrent Blocker enforcement logs.'
        }
        icon={<ShieldAlert className="h-4 w-4" />}
        actions={
          <Button variant="outline" size="sm" onClick={handleRefresh}>
            <RefreshCw className="h-4 w-4" />
            <span>{isRu ? 'Обновить' : 'Refresh'}</span>
          </Button>
        }
      />

      {/* 3-Card Clickable Inspector KPI Ribbon */}
      <div className="grid gap-3 sm:grid-cols-3">
        <button
          type="button"
          onClick={() => setTab('srh')}
          className={cn(
            'node-blade relative overflow-hidden rounded-xl border p-2 pl-3.5 text-left transition-all',
            tab === 'srh' ? 'border-primary/60 ring-1 ring-primary/25' : 'border-border/80 hover:border-primary/40',
          )}
        >
          <div className="absolute inset-y-0 left-0 w-1 bg-emerald-400" aria-hidden="true" />
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Запросы подписок (SRH)' : 'Subscription Requests (SRH)'}</span>
            <FileText className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="mt-1.5 font-mono text-2xl font-bold tabular-nums">
            {srhData?.total ?? srhEntries.length}
          </div>
          <SplitBar
            className="mt-2"
            segments={[
              { pct: srhOkPct, className: 'bg-emerald-400' },
              { pct: 100 - srhOkPct, className: 'bg-red-500' },
            ]}
          />
        </button>

        <button
          type="button"
          onClick={() => setTab('hwid')}
          className={cn(
            'node-blade relative overflow-hidden rounded-xl border p-2 pl-3.5 text-left transition-all',
            tab === 'hwid' ? 'border-primary/60 ring-1 ring-primary/25' : 'border-border/80 hover:border-primary/40',
          )}
        >
          <div className="absolute inset-y-0 left-0 w-1 bg-cyan-400" aria-hidden="true" />
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Реестр HWID устройств' : 'HWID Device Registry'}</span>
            <Smartphone className="h-4 w-4 text-cyan-400" />
          </div>
          <div className="mt-1.5 flex items-baseline justify-between">
            <span className="font-mono text-2xl font-bold tabular-nums">
              {hwidData?.total ?? hwidEntries.length}
            </span>
            {hwidCollisionSet.size > 0 && (
              <Badge variant="warning" className="text-[10px]">
                {hwidCollisionSet.size} {isRu ? 'коллизий' : 'collisions'}
              </Badge>
            )}
          </div>
          <div className="mt-1.5 text-[11px] text-muted-foreground">
            {isRu ? 'Привязка аппаратных идентификаторов' : 'Hardware ID device bindings'}
          </div>
        </button>

        <button
          type="button"
          onClick={() => setTab('torrent')}
          className={cn(
            'node-blade relative overflow-hidden rounded-xl border p-2 pl-3.5 text-left transition-all',
            tab === 'torrent' ? 'border-primary/60 ring-1 ring-primary/25' : 'border-border/80 hover:border-primary/40',
          )}
        >
          <div className="absolute inset-y-0 left-0 w-1 bg-amber-400" aria-hidden="true" />
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Блокировщик торрентов' : 'Torrent Blocker'}</span>
            <ShieldAlert className="h-4 w-4 text-amber-400" />
          </div>
          <div className="mt-1.5 font-mono text-2xl font-bold tabular-nums">
            {torrentData?.total ?? torrentEntries.length}
          </div>
          <div className="mt-1.5 text-[11px] text-muted-foreground">
            {isRu ? 'Срабатывания правил BitTorrent' : 'BitTorrent enforcement events'}
          </div>
        </button>
      </div>

      <Tabs value={tab} onValueChange={(v) => setTab(v as InspectorTab)}>
        <TabsList>
          <TabsTrigger value="srh">
            <FileText className="mr-1.5 h-3.5 w-3.5" />
            {isRu ? 'История подписок (SRH)' : 'Subscription Request History'} (
            {srhData?.total ?? srhEntries.length})
          </TabsTrigger>
          <TabsTrigger value="hwid">
            <Smartphone className="mr-1.5 h-3.5 w-3.5" />
            {isRu ? 'Инспектор HWID' : 'HWID Inspector'} ({hwidData?.total ?? hwidEntries.length})
          </TabsTrigger>
          <TabsTrigger value="torrent">
            <ShieldAlert className="mr-1.5 h-3.5 w-3.5" />
            {isRu ? 'Блокировки торрентов' : 'Torrent Blocker Reports'} (
            {torrentData?.total ?? torrentEntries.length})
          </TabsTrigger>
        </TabsList>

        <TabsContent value="srh" className="space-y-4">
          <div className="relative max-w-md">
            <Search className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder={
                isRu
                  ? 'Поиск SRH по клиенту, IP, User-Agent, HWID, правилу SRR...'
                  : 'Filter SRH by client, IP, User-Agent, HWID, SRR rule...'
              }
              value={srhSearch}
              onChange={(e) => setSrhSearch(e.target.value)}
              className="pl-9"
            />
          </div>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm">
                {isRu ? 'Журнал запросов подписок (SRH)' : 'Subscription Request History (SRH)'}
              </CardTitle>
            </CardHeader>
            <CardContent>
              {srhLoading ? (
                <div className="space-y-2 py-2">
                  <Skeleton className="h-9 w-full" />
                  <Skeleton className="h-9 w-full" />
                  <Skeleton className="h-9 w-full" />
                </div>
              ) : srhEntries.length === 0 ? (
                <RackEmptyState
                  icon={FileText}
                  title={isRu ? 'Записей запросов подписок пока нет' : 'No subscription request records found'}
                  description={
                    isRu
                      ? 'Каждый запрос клиентского приложения по ссылке подписки фиксируется здесь.'
                      : 'Every subscription fetch request from client apps is logged here.'
                  }
                />
              ) : (
                <div className="overflow-x-auto">
                  <table className="data-table w-full text-xs">
                    <thead className="bg-muted/30">
                      <tr className="border-b text-left text-muted-foreground">
                        <th className="py-1.5 px-2">{isRu ? 'Время' : 'Time'}</th>
                        <th className="py-1.5 px-2">{isRu ? 'Клиент' : 'Client'}</th>
                        <th className="py-1.5 px-2">IP</th>
                        <th className="py-1.5 px-2">{isRu ? 'Приложение / User-Agent' : 'Detected App / User-Agent'}</th>
                        <th className="py-1.5 px-2">{isRu ? 'Формат / Правило SRR' : 'Format / SRR Rule'}</th>
                        <th className="py-1.5 px-2">HWID</th>
                        <th className="py-1.5 px-2">{isRu ? 'HTTP Статус' : 'HTTP Status'}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {srhEntries.map((item) => (
                        <tr key={item.id} className="border-b last:border-b-0 hover:bg-muted/20">
                          <td className="py-1.5 px-2 font-mono text-muted-foreground">
                            {formatDateTime(item.createdAt, timezone)}
                          </td>
                          <td className="py-1.5 px-2 font-semibold">
                            {item.username || item.clientId || '—'}
                          </td>
                          <td className="py-1.5 px-2 font-mono">{item.ip}</td>
                          <td className="py-1.5 px-2">
                            {item.detectedApp && (
                              <Badge variant="outline" className="mr-1.5">
                                {item.detectedApp}
                              </Badge>
                            )}
                            <span className="font-mono text-muted-foreground break-all">
                              {item.userAgent}
                            </span>
                          </td>
                          <td className="py-1.5 px-2">
                            {item.format && <Badge variant="secondary">{item.format}</Badge>}
                            {item.srrRuleName && (
                              <span className="ml-1.5 text-muted-foreground">({item.srrRuleName})</span>
                            )}
                          </td>
                          <td className="py-1.5 px-2 font-mono text-muted-foreground">
                            {item.hwid || '—'}
                          </td>
                          <td className="py-1.5 px-2">
                            <Badge variant={item.status >= 200 && item.status < 300 ? 'success' : 'destructive'}>
                              {item.status}
                            </Badge>
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

        <TabsContent value="hwid" className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="relative flex-1 max-w-md">
              <Search className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
              <Input
                placeholder={
                  isRu
                    ? 'Поиск по HWID, клиенту, ОС, модели, IP...'
                    : 'Search HWID, client, OS, model, IP...'
                }
                value={hwidSearch}
                onChange={(e) => setHwidSearch(e.target.value)}
                className="pl-9"
              />
            </div>
            {hwidCollisionSet.size > 0 && (
              <Badge variant="warning" className="gap-1">
                <AlertTriangle className="h-3.5 w-3.5" />
                {isRu
                  ? `Обнаружено общих HWID у разных клиентов: ${hwidCollisionSet.size}`
                  : `${hwidCollisionSet.size} multi-client HWID collision(s) detected`}
              </Badge>
            )}
          </div>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm">
                {isRu ? 'Зарегистрированные HWID-устройства' : 'Registered HWID Devices'}
              </CardTitle>
            </CardHeader>
            <CardContent>
              {hwidLoading ? (
                <div className="space-y-2 py-2">
                  <Skeleton className="h-9 w-full" />
                  <Skeleton className="h-9 w-full" />
                  <Skeleton className="h-9 w-full" />
                </div>
              ) : hwidEntries.length === 0 ? (
                <RackEmptyState
                  icon={Smartphone}
                  title={isRu ? 'HWID-устройства ещё не зарегистрированы' : 'No HWID devices registered yet'}
                  description={
                    isRu
                      ? 'Аппаратные идентификаторы сохраняются при подключении клиентов из приложений с поддержкой HWID.'
                      : 'Hardware IDs are captured when clients fetch subscriptions from HWID-enabled apps.'
                  }
                />
              ) : (
                <div className="overflow-x-auto">
                  <table className="data-table w-full text-xs">
                    <thead className="bg-muted/30">
                      <tr className="border-b text-left text-muted-foreground">
                        <th className="py-1.5 px-2">HWID</th>
                        <th className="py-1.5 px-2">{isRu ? 'Клиент' : 'Client'}</th>
                        <th className="py-1.5 px-2">{isRu ? 'ОС / Модель' : 'OS / Model'}</th>
                        <th className="py-1.5 px-2">IP</th>
                        <th className="py-1.5 px-2">{isRu ? 'Активность' : 'Last Seen'}</th>
                        <th className="py-1.5 px-2 text-right">{isRu ? 'Действие' : 'Action'}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {hwidEntries.map((dev) => {
                        const isCollision = hwidCollisionSet.has(dev.hwid);
                        return (
                          <tr key={dev.id} className="border-b last:border-b-0 hover:bg-muted/20">
                            <td className="py-1.5 px-2 font-mono">
                              <div className="flex items-center gap-1.5">
                                <span>{dev.hwid}</span>
                                {isCollision && (
                                  <Badge variant="warning" className="text-[10px]">
                                    {isRu ? 'Общий HWID' : 'Shared HWID'}
                                  </Badge>
                                )}
                              </div>
                            </td>
                            <td className="py-1.5 px-2 font-semibold">
                              {dev.username || dev.clientId}
                              {dev.deviceLimit != null && (
                                <span className="ml-1.5 text-muted-foreground">
                                  ({isRu ? 'лимит' : 'limit'}: {dev.deviceLimit})
                                </span>
                              )}
                            </td>
                            <td className="py-1.5 px-2">
                              {[dev.os, dev.osVersion, dev.model].filter(Boolean).join(' ') ||
                                dev.userAgent ||
                                '—'}
                            </td>
                            <td className="py-1.5 px-2 font-mono">{dev.ip || '—'}</td>
                            <td className="py-1.5 px-2 font-mono text-muted-foreground">
                              {formatDateTime(dev.lastSeenAt, timezone)}
                            </td>
                            <td className="py-1.5 px-2 text-right">
                              <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={() =>
                                  revokeDeviceMut.mutate({ clientId: dev.clientId, deviceId: dev.id })
                                }
                                loading={revokeDeviceMut.isPending}
                              >
                                <Trash2 className="h-3.5 w-3.5 text-red-400" />
                                <span>{isRu ? 'Отозвать' : 'Revoke'}</span>
                              </Button>
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="torrent" className="space-y-4">
          <div className="relative max-w-md">
            <Search className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder={
                isRu
                  ? 'Поиск блокировок торрентов по клиенту, ноде, IP, правилу...'
                  : 'Search torrent blocker reports by client, node, IP, rule...'
              }
              value={torrentSearch}
              onChange={(e) => setTorrentSearch(e.target.value)}
              className="pl-9"
            />
          </div>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm">
                {isRu ? 'Отчёты блокировщика торрентов' : 'Torrent Blocker Reports'}
              </CardTitle>
            </CardHeader>
            <CardContent>
              {torrentLoading ? (
                <div className="space-y-2 py-2">
                  <Skeleton className="h-9 w-full" />
                  <Skeleton className="h-9 w-full" />
                  <Skeleton className="h-9 w-full" />
                </div>
              ) : filteredTorrentEntries.length === 0 ? (
                <RackEmptyState
                  icon={ShieldAlert}
                  title={isRu ? 'Событий блокировки BitTorrent не зафиксировано' : 'No BitTorrent block events recorded'}
                  description={
                    isRu
                      ? 'При попытке использования P2P/BitTorrent через защищённые инбаунды события появятся здесь.'
                      : 'When BitTorrent traffic is intercepted on protected inbounds, reports will appear here.'
                  }
                />
              ) : (
                <div className="overflow-x-auto">
                  <table className="data-table w-full text-xs">
                    <thead className="bg-muted/30">
                      <tr className="border-b text-left text-muted-foreground">
                        <th className="py-1.5 px-2">{isRu ? 'Время' : 'Timestamp'}</th>
                        <th className="py-1.5 px-2">{isRu ? 'Нода' : 'Node'}</th>
                        <th className="py-1.5 px-2">{isRu ? 'Клиент' : 'Client'}</th>
                        <th className="py-1.5 px-2">IP</th>
                        <th className="py-1.5 px-2">{isRu ? 'Протокол / Правило' : 'Protocol / Rule'}</th>
                        <th className="py-1.5 px-2">{isRu ? 'Действие' : 'Action'}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {filteredTorrentEntries.map((rep) => (
                        <tr key={rep.id} className="border-b last:border-b-0 hover:bg-muted/20">
                          <td className="py-1.5 px-2 font-mono text-muted-foreground">
                            {formatDateTime(rep.createdAt, timezone)}
                          </td>
                          <td className="py-1.5 px-2">{rep.nodeName || rep.nodeId || '—'}</td>
                          <td className="py-1.5 px-2 font-semibold">
                            {rep.username || rep.clientId || '—'}
                          </td>
                          <td className="py-1.5 px-2 font-mono">{rep.sourceIp}</td>
                          <td className="py-1.5 px-2">
                            {rep.protocol && <Badge variant="outline">{rep.protocol}</Badge>}
                            {rep.rule && <span className="ml-1.5 font-mono">{rep.rule}</span>}
                          </td>
                          <td className="py-1.5 px-2">
                            <Badge variant="destructive">{rep.action || 'BLOCKED'}</Badge>
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
    </div>
  );
}
