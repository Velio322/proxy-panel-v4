import { useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Cpu,
  Edit,
  FileCode2,
  MoreHorizontal,
  Network,
  Plus,
  Power,
  Search,
  Server,
  Shield,
  Trash2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Skeleton } from '@/components/ui/skeleton';
import {
  PageHero,
  RackEmptyState,
  SplitBar,
  StatusFilterPills,
  ViewModeToggle,
} from '@/components/ui/PagePrimitives';
import { InboundModal } from '@/components/inbounds/InboundModal';
import { InboundBladeCard } from '@/components/inbounds/InboundBladeCard';
import { XrayConfigStudioModal } from '@/components/inbounds/XrayConfigStudioModal';
import { getApiErrorMessage, inboundsApi, nodesApi } from '@/lib/api';
import { PROTOCOL_COLORS, PROTOCOLS, cn, getCoreForProtocol } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { Inbound } from '@/types';

export function InboundsPage() {
  const { t } = useTranslation();
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';
  const qc = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();

  const [viewMode, setViewMode] = useState<'blades' | 'table'>('blades');
  const [statusFilter, setStatusFilter] = useState<'all' | 'enabled' | 'disabled'>('all');
  const [nodeFilter, setNodeFilter] = useState<string>('all');
  const [protocolFilter, setProtocolFilter] = useState<string>('all');
  const [search, setSearch] = useState('');
  const [modalOpen, setModalOpen] = useState(false);
  const [studioOpen, setStudioOpen] = useState(false);
  const [editing, setEditing] = useState<Inbound | null>(null);

  useEffect(() => {
    if (searchParams.get('create') === '1') {
      setEditing(null);
      setModalOpen(true);
      const next = new URLSearchParams(searchParams);
      next.delete('create');
      setSearchParams(next, { replace: true });
    }
  }, [searchParams, setSearchParams]);

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
  });

  const { data: inbounds = [], isLoading } = useQuery({
    queryKey: ['inbounds', nodeFilter, protocolFilter],
    queryFn: () =>
      inboundsApi.list({
        nodeId: nodeFilter === 'all' ? undefined : nodeFilter,
        protocol: protocolFilter === 'all' ? undefined : protocolFilter,
      }),
  });

  const toggleMut = useMutation({
    mutationFn: (id: string) => inboundsApi.toggle(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['inbounds'] });
      toast({ title: isRu ? 'Статус протокола изменён' : 'Inbound toggled', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка' : 'Toggle failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => inboundsApi.delete(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['inbounds'] });
      toast({ title: isRu ? 'Протокол удалён' : 'Inbound deleted', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка удаления' : 'Delete failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const stats = useMemo(() => {
    let enabled = 0;
    let disabled = 0;
    let totalShares = 0;
    let sniffingCount = 0;
    const nodeIds = new Set<string>();
    const cores: Record<string, number> = { xray: 0, singbox: 0, naive: 0, mieru: 0 };

    for (const ib of inbounds) {
      if (ib.enable) enabled += 1;
      else disabled += 1;
      totalShares += ib.portShares?.length ?? 0;
      if (ib.sniffing) sniffingCount += 1;
      nodeIds.add(ib.nodeId);
      const c = getCoreForProtocol(ib.protocol);
      cores[c] = (cores[c] ?? 0) + 1;
    }

    return {
      total: inbounds.length,
      enabled,
      disabled,
      totalShares,
      sniffingCount,
      coveredNodes: nodeIds.size,
      cores,
    };
  }, [inbounds]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return inbounds.filter((i) => {
      if (statusFilter === 'enabled' && !i.enable) return false;
      if (statusFilter === 'disabled' && i.enable) return false;
      if (!q) return true;
      return (
        i.tag.toLowerCase().includes(q) ||
        i.protocol.toLowerCase().includes(q) ||
        String(i.port).includes(q) ||
        (i.node?.name ?? '').toLowerCase().includes(q)
      );
    });
  }, [inbounds, search, statusFilter]);

  const statusPills = [
    { key: 'all', label: isRu ? 'Все' : 'All', count: stats.total },
    {
      key: 'enabled',
      label: t('common.enabled'),
      count: stats.enabled,
      dotClass: 'bg-emerald-400',
    },
    {
      key: 'disabled',
      label: t('common.disabled'),
      count: stats.disabled,
      dotClass: 'bg-zinc-500',
    },
  ];

  return (
    <div className="space-y-5">
      <PageHero
        title={t('inbounds.title')}
        subtitle={t('inbounds.subtitle')}
        icon={<Network className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs tabular-nums">
            {stats.enabled}/{stats.total} {isRu ? 'активны' : 'enabled'}
          </Badge>
        }
        actions={
          <>
            <ViewModeToggle value={viewMode} onChange={setViewMode} isRu={isRu} />
            <Button
              variant="outline"
              size="sm"
              onClick={() => setStudioOpen(true)}
              disabled={nodes.length === 0}
            >
              <FileCode2 className="h-4 w-4" />
              {t('inbounds.studio')}
            </Button>
            <Button
              size="sm"
              onClick={() => {
                setEditing(null);
                setModalOpen(true);
              }}
            >
              <Plus className="h-4 w-4" />
              {t('inbounds.addInbound')}
            </Button>
          </>
        }
      />

      {/* 4-Card Protocols KPI Ribbon */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Card className="p-4 space-y-2.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Протоколы (Inbounds)' : 'Total Inbounds'}</span>
            <Network className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="flex items-baseline justify-between">
            <span className="font-mono text-2xl font-bold tabular-nums">
              {stats.enabled} / {stats.total}
            </span>
            <span className="text-xs text-muted-foreground">
              {stats.disabled} {isRu ? 'откл.' : 'off'}
            </span>
          </div>
          <SplitBar
            segments={[
              { pct: stats.total > 0 ? (stats.enabled / stats.total) * 100 : 100, className: 'bg-emerald-400' },
              { pct: stats.total > 0 ? (stats.disabled / stats.total) * 100 : 0, className: 'bg-zinc-600' },
            ]}
          />
        </Card>

        <Card className="p-4 space-y-2.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Распределение по ядрам' : 'Core Engines'}</span>
            <Cpu className="h-4 w-4 text-cyan-400" />
          </div>
          <div className="grid grid-cols-4 gap-1.5 pt-0.5 text-center font-mono text-xs">
            <div className="rack-inset rounded-md py-1.5">
              <div className="text-[10px] text-muted-foreground">Xray</div>
              <div className="font-bold">{stats.cores.xray}</div>
            </div>
            <div className="rack-inset rounded-md py-1.5">
              <div className="text-[10px] text-muted-foreground">SB</div>
              <div className="font-bold">{stats.cores.singbox}</div>
            </div>
            <div className="rack-inset rounded-md py-1.5">
              <div className="text-[10px] text-muted-foreground">Naive</div>
              <div className="font-bold">{stats.cores.naive}</div>
            </div>
            <div className="rack-inset rounded-md py-1.5">
              <div className="text-[10px] text-muted-foreground">Mieru</div>
              <div className="font-bold">{stats.cores.mieru}</div>
            </div>
          </div>
        </Card>

        <Card className="p-4 space-y-2.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Покрытие нод' : 'Node Coverage'}</span>
            <Server className="h-4 w-4 text-primary" />
          </div>
          <div className="flex items-baseline justify-between">
            <span className="font-mono text-2xl font-bold tabular-nums">
              {stats.coveredNodes} / {nodes.length}
            </span>
            <span className="text-xs text-muted-foreground">
              {isRu ? 'нод с протоколами' : 'nodes configured'}
            </span>
          </div>
          <SplitBar
            segments={[
              {
                pct: nodes.length > 0 ? (stats.coveredNodes / nodes.length) * 100 : 100,
                className: 'bg-primary',
              },
            ]}
          />
        </Card>

        <Card className="p-4 space-y-2.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Sniffing и Port-Share' : 'Sniffing & Port-Share'}</span>
            <Shield className="h-4 w-4 text-amber-400" />
          </div>
          <div className="flex items-baseline justify-between">
            <span className="font-mono text-2xl font-bold tabular-nums">{stats.sniffingCount}</span>
            <span className="text-xs text-muted-foreground">
              {isRu ? `sniffing · ${stats.totalShares} доп. портов` : `sniffing · ${stats.totalShares} shares`}
            </span>
          </div>
          <div className="text-[11px] text-muted-foreground">
            {isRu ? 'Авто-детекция TLS/HTTP/QUIC трафика' : 'Deep packet protocol detection active'}
          </div>
        </Card>
      </div>

      {/* Filter Toolbar */}
      <Card>
        <CardHeader className="py-3.5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <StatusFilterPills
              items={statusPills}
              value={statusFilter}
              onChange={(v) => setStatusFilter(v as 'all' | 'enabled' | 'disabled')}
            />

            <div className="flex flex-wrap items-center gap-2.5 flex-1 justify-end min-w-[260px]">
              <div className="relative flex-1 min-w-[200px] max-w-sm">
                <Search className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
                <Input
                  placeholder={t('common.search')}
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  className="pl-9"
                />
              </div>
              <div className="w-44">
                <Select value={nodeFilter} onValueChange={setNodeFilter}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">{t('clients.allNodesFilter')}</SelectItem>
                    {nodes.map((n) => (
                      <SelectItem key={n.id} value={n.id}>
                        {n.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="w-44">
                <Select value={protocolFilter} onValueChange={setProtocolFilter}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">{t('clients.allProtocols')}</SelectItem>
                    {PROTOCOLS.map((p) => (
                      <SelectItem key={p} value={p}>
                        {p}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>
          </div>
        </CardHeader>
      </Card>

      {/* Main Content */}
      {isLoading ? (
        <div className="grid gap-4 xl:grid-cols-2">
          <Skeleton className="h-40 w-full rounded-xl" />
          <Skeleton className="h-40 w-full rounded-xl" />
        </div>
      ) : filtered.length === 0 ? (
        <RackEmptyState
          icon={Network}
          title={t('common.noData')}
          description={
            isRu
              ? 'Добавьте первый входной протокол (VLESS, Hysteria2, NaiveProxy, Mieru и др.) на серверную ноду.'
              : 'Configure your first inbound protocol endpoint on a proxy node.'
          }
          action={
            <Button
              size="sm"
              onClick={() => {
                setEditing(null);
                setModalOpen(true);
              }}
            >
              <Plus className="h-4 w-4" />
              {t('inbounds.addInbound')}
            </Button>
          }
        />
      ) : viewMode === 'blades' ? (
        <div className="grid gap-4 xl:grid-cols-2">
          {filtered.map((ib) => (
            <InboundBladeCard
              key={ib.id}
              inbound={ib}
              isRu={isRu}
              onEdit={(target) => {
                setEditing(target);
                setModalOpen(true);
              }}
              onToggle={(id) => toggleMut.mutate(id)}
              onDelete={(id) => deleteMut.mutate(id)}
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
                    <th className="py-2.5 px-3">{isRu ? 'Тег / Название' : 'Tag'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Протокол' : 'Protocol'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Ядро' : 'Engine'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Нода' : 'Node'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Адрес : Порт' : 'Listen : Port'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Доп. порты' : 'Shares'}</th>
                    <th className="py-2.5 px-3">{t('common.status')}</th>
                    <th className="py-2.5 px-3 text-right">{t('common.actions')}</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((ib) => (
                    <tr key={ib.id} className="border-b border-border/60 last:border-b-0 hover:bg-accent/35">
                      <td className="py-2.5 px-3 font-medium">
                        <button
                          type="button"
                          onClick={() => {
                            setEditing(ib);
                            setModalOpen(true);
                          }}
                          className="flex items-center gap-2 hover:text-primary transition-colors text-left"
                        >
                          <Network className="h-4 w-4 text-primary shrink-0" />
                          <span>{ib.tag}</span>
                        </button>
                      </td>
                      <td className="py-2.5 px-3">
                        <Badge className={cn('border', PROTOCOL_COLORS[ib.protocol])}>{ib.protocol}</Badge>
                      </td>
                      <td className="py-2.5 px-3 font-mono text-xs text-muted-foreground">
                        {getCoreForProtocol(ib.protocol)}
                      </td>
                      <td className="py-2.5 px-3 text-xs text-muted-foreground">
                        {ib.node?.name ?? ib.nodeId}
                      </td>
                      <td className="py-2.5 px-3 font-mono text-xs">
                        {ib.listen}:{ib.port}
                      </td>
                      <td className="py-2.5 px-3">
                        <Badge variant="outline">{ib.portShares?.length ?? 0}</Badge>
                      </td>
                      <td className="py-2.5 px-3">
                        <Badge variant={ib.enable ? 'success' : 'secondary'}>
                          {ib.enable ? t('common.enabled') : t('common.disabled')}
                        </Badge>
                      </td>
                      <td className="py-2.5 px-3 text-right">
                        <div className="inline-flex items-center gap-1.5">
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="h-8 gap-1.5 px-2.5 text-xs"
                            onClick={() => {
                              setEditing(ib);
                              setModalOpen(true);
                            }}
                          >
                            <Edit className="h-3.5 w-3.5" />
                            <span>{t('common.edit')}</span>
                          </Button>
                          <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            className="h-8 gap-1 px-2.5 text-xs"
                            onClick={() => toggleMut.mutate(ib.id)}
                          >
                            <Power className="h-3.5 w-3.5" />
                            <span>{ib.enable ? t('inbounds.turnOff') : t('inbounds.turnOn')}</span>
                          </Button>
                          <DropdownMenu>
                            <DropdownMenuTrigger asChild>
                              <Button type="button" variant="ghost" size="icon" className="h-8 w-8" aria-label={t('common.actions')}>
                                <MoreHorizontal className="h-4 w-4" />
                              </Button>
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="end" className="w-44">
                              <DropdownMenuItem
                                onClick={() => {
                                  setEditing(ib);
                                  setModalOpen(true);
                                }}
                              >
                                <Edit className="h-3.5 w-3.5" />
                                <span>{t('common.edit')}</span>
                              </DropdownMenuItem>
                              <DropdownMenuItem onClick={() => toggleMut.mutate(ib.id)}>
                                <Power className="h-3.5 w-3.5" />
                                <span>{ib.enable ? t('inbounds.turnOff') : t('inbounds.turnOn')}</span>
                              </DropdownMenuItem>
                              <DropdownMenuSeparator />
                              <DropdownMenuItem
                                onClick={() => deleteMut.mutate(ib.id)}
                                className="text-red-500 dark:text-red-400 focus:text-red-500 dark:focus:text-red-400"
                              >
                                <Trash2 className="h-3.5 w-3.5" />
                                <span>{t('common.delete')}</span>
                              </DropdownMenuItem>
                            </DropdownMenuContent>
                          </DropdownMenu>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}

      <InboundModal
        open={modalOpen}
        onOpenChange={setModalOpen}
        inbound={editing}
        nodes={nodes}
        defaultNodeId={nodeFilter !== 'all' ? nodeFilter : undefined}
        onSaved={() => qc.invalidateQueries({ queryKey: ['inbounds'] })}
      />

      <XrayConfigStudioModal
        open={studioOpen}
        onOpenChange={setStudioOpen}
        nodes={nodes}
        initialNodeId={nodeFilter !== 'all' ? nodeFilter : undefined}
        onSaved={() => {
          qc.invalidateQueries({ queryKey: ['inbounds'] });
          qc.invalidateQueries({ queryKey: ['nodes'] });
        }}
      />
    </div>
  );
}
