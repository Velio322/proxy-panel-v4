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
  StatusFilterPills,
  ViewModeToggle,
} from '@/components/ui/PagePrimitives';
import { InboundModal } from '@/components/inbounds/InboundModal';
import { InboundBladeCard } from '@/components/inbounds/InboundBladeCard';
import { extractProtocolCardMeta } from '@/components/inbounds/protocolCardMeta';
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
    const cores: Record<string, number> = { xray: 0, singbox: 0, naive: 0, mieru: 0, telemt: 0 };

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
      return extractProtocolCardMeta(i, isRu).searchText.includes(q);
    });
  }, [inbounds, search, statusFilter, isRu]);

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

      {/* Compact KPI strip */}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded-xl border border-border/80 bg-background/60 px-3 py-2 text-[11px]">
        <span className="inline-flex items-center gap-1 font-mono tabular-nums">
          <Network className="h-3 w-3 text-emerald-400" />
          <span className="font-bold text-foreground">{stats.enabled}</span>
          <span className="text-muted-foreground">/ {stats.total} {isRu ? 'активны' : 'enabled'}</span>
          {stats.disabled > 0 && <span className="text-zinc-500">· {stats.disabled} {isRu ? 'откл.' : 'off'}</span>}
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Cpu className="h-3 w-3 text-cyan-400" />
          Xray <span className="font-bold text-foreground">{stats.cores.xray}</span>
          <span>· SB {stats.cores.singbox}</span>
          <span>· Naive {stats.cores.naive}</span>
          <span>· Mieru {stats.cores.mieru}</span>
          <span>· TG {stats.cores.telemt}</span>
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Server className="h-3 w-3 text-primary" />
          <span className="text-foreground">{stats.coveredNodes}</span>/ {nodes.length} {isRu ? 'нод с протоколами' : 'nodes covered'}
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Shield className="h-3 w-3 text-amber-400" />
          <span className="text-foreground">{stats.sniffingCount}</span> {isRu ? 'sniffing' : 'sniffing'}
          <span>· {stats.totalShares} {isRu ? 'доп. портов' : 'shares'}</span>
        </span>
      </div>

      {/* Filter Toolbar */}
      <Card>
        <CardHeader className="py-2.5">
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
        <div className="grid gap-3 xl:grid-cols-2 2xl:grid-cols-3">
          <Skeleton className="h-24 w-full rounded-xl" />
          <Skeleton className="h-24 w-full rounded-xl" />
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
        <div className="grid gap-3 xl:grid-cols-2 2xl:grid-cols-3">
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
                    <th className="py-1.5 px-2">{isRu ? 'Тег / Название' : 'Tag'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Протокол' : 'Protocol'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Ядро' : 'Engine'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Нода' : 'Node'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Адрес / Эндпоинт' : 'Endpoint'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Конфигурация' : 'Config'}</th>
                    <th className="py-1.5 px-2">{t('common.status')}</th>
                    <th className="py-1.5 px-2 text-right">{t('common.actions')}</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((ib) => {
                    const meta = extractProtocolCardMeta(ib, isRu);
                    return (
                      <tr key={ib.id} className="border-b border-border/60 last:border-b-0 hover:bg-accent/35">
                        <td className="py-1.5 px-2 font-medium">
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
                        <td className="py-1.5 px-2">
                          <div className="flex flex-col items-start gap-0.5">
                            <Badge className={cn('border', PROTOCOL_COLORS[ib.protocol])}>
                              {meta.protocolLabel}
                            </Badge>
                            <span className="font-mono text-[10px] text-muted-foreground">
                              {meta.subtitle}
                            </span>
                          </div>
                        </td>
                        <td className="py-1.5 px-2 font-mono text-xs text-muted-foreground">
                          {meta.coreLabel}
                        </td>
                        <td className="py-1.5 px-2 text-xs text-muted-foreground">
                          {ib.node?.name ?? ib.nodeId}
                        </td>
                        <td className="py-1.5 px-2 font-mono text-xs">
                          <div className="flex flex-wrap items-center gap-1">
                            <span>{meta.endpoint.display}</span>
                            {meta.endpoint.secondary && (
                              <span className="rounded border border-border/60 bg-background/60 px-1 py-0 text-[10px] text-muted-foreground">
                                {meta.endpoint.secondary.label}:{meta.endpoint.secondary.value}
                              </span>
                            )}
                          </div>
                        </td>
                        <td className="py-1.5 px-2 max-w-[18rem]">
                          <div className="flex flex-wrap items-center gap-1">
                            <span
                              className="truncate font-mono text-[11px] text-muted-foreground"
                              title={meta.compactSummary}
                            >
                              {meta.compactSummary || '—'}
                            </span>
                            {(ib.portShares?.length ?? 0) > 0 && (
                              <Badge variant="outline" className="text-[10px]">
                                +{ib.portShares!.length} {isRu ? 'доп.' : 'shares'}
                              </Badge>
                            )}
                          </div>
                        </td>
                        <td className="py-1.5 px-2">
                          <Badge variant={ib.enable ? 'success' : 'secondary'}>
                            {ib.enable ? t('common.enabled') : t('common.disabled')}
                          </Badge>
                        </td>
                        <td className="py-1.5 px-2 text-right">
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
                    );
                  })}
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
