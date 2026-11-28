import { useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Activity,
  ArrowRightLeft,
  Ban,
  CheckCircle2,
  Plus,
  RotateCcw,
  Search,
  Server,
  ShieldAlert,
  Trash2,
  Upload,
  Users,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Checkbox } from '@/components/ui/checkbox';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Skeleton } from '@/components/ui/skeleton';
import {
  PageHero,
  RackEmptyState,
  StatusFilterPills,
  ViewModeToggle,
} from '@/components/ui/PagePrimitives';
import { CreateClientModal } from '@/components/modals/CreateClientModal';
import { EditClientModal, type EditClientTab } from '@/components/modals/EditClientModal';
import { SubscriptionModal } from '@/components/modals/SubscriptionModal';
import { CsvImportModal } from '@/components/modals/CsvImportModal';
import { MigratePanelModal } from '@/components/modals/MigratePanelModal';
import { BulkAssignNodesModal } from '@/components/modals/BulkAssignNodesModal';
import { ClientRow } from '@/pages/clients/components/ClientRow';
import { ClientBladeCard } from '@/pages/clients/components/ClientBladeCard';
import { clientsApi, dashboardApi, getApiErrorMessage, nodesApi, plansApi } from '@/lib/api';
import { PROTOCOLS, cn, formatBytes } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { Client } from '@/types';
import { useCopyFeedback } from '@/hooks/use-copy-feedback';

export function ClientsPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';
  const [searchParams, setSearchParams] = useSearchParams();

  const [viewMode, setViewMode] = useState<'blades' | 'table'>('blades');
  const [search, setSearch] = useState('');
  const [bannedFilter, setBannedFilter] = useState('all');
  const [protocolFilter, setProtocolFilter] = useState('all');
  const [nodeFilter, setNodeFilter] = useState('all');
  const [page, setPage] = useState(1);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [copiedUuidId, copyUuidId] = useCopyFeedback();
  const [bulkPlanId, setBulkPlanId] = useState('none');

  const [createOpen, setCreateOpen] = useState(false);
  const [csvOpen, setCsvOpen] = useState(false);
  const [migrateOpen, setMigrateOpen] = useState(false);
  const [bulkNodesOpen, setBulkNodesOpen] = useState(false);
  const [editingClient, setEditingClient] = useState<Client | null>(null);
  const [editingTab, setEditingTab] = useState<EditClientTab>('profile');
  const [subClient, setSubClient] = useState<Client | null>(null);

  useEffect(() => {
    if (searchParams.get('create') === '1') {
      setCreateOpen(true);
      const next = new URLSearchParams(searchParams);
      next.delete('create');
      setSearchParams(next, { replace: true });
    }
  }, [searchParams, setSearchParams]);

  const { data: overview } = useQuery({
    queryKey: ['dashboard-overview'],
    queryFn: () => dashboardApi.overview(),
    refetchInterval: 30_000,
  });

  const { data: plans = [] } = useQuery({
    queryKey: ['plans'],
    queryFn: () => plansApi.list(),
  });

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
  });

  const sortedNodes = useMemo(
    () => [...nodes].sort((a, b) => a.name.localeCompare(b.name)),
    [nodes],
  );

  const [debouncedSearch, setDebouncedSearch] = useState(search);
  useEffect(() => {
    const id = window.setTimeout(() => setDebouncedSearch(search), 250);
    return () => window.clearTimeout(id);
  }, [search]);

  useEffect(() => {
    setSelectedIds([]);
  }, [debouncedSearch, bannedFilter, protocolFilter, nodeFilter, page]);

  const { data: res, isLoading } = useQuery({
    queryKey: ['clients', debouncedSearch, bannedFilter, protocolFilter, nodeFilter, page],
    queryFn: () =>
      clientsApi.list({
        search: debouncedSearch.trim() || undefined,
        banned: bannedFilter === 'all' ? undefined : bannedFilter,
        protocol: protocolFilter === 'all' ? undefined : protocolFilter,
        nodeId: nodeFilter === 'all' ? undefined : nodeFilter,
        page,
        limit: 20,
      }),
  });

  const clients = res?.data ?? [];
  const totalPages = res?.pages ?? 1;
  const totalCount = res?.total ?? clients.length;

  const clientStats = useMemo(() => {
    let totalUsed = 0;
    let totalDl = 0;
    let totalUl = 0;
    let allNodesCount = 0;
    let pinnedCount = 0;
    let noNodesCount = 0;
    let expiringOrHighQuota = 0;
    const now = Date.now();

    for (const c of clients) {
      totalUsed += Number(c.usedTraffic || 0);
      totalDl += Number(c.downloadTraffic || 0);
      totalUl += Number(c.uploadTraffic || 0);
      if (c.allNodes !== false) allNodesCount += 1;
      else if ((c.nodes?.length ?? 0) > 0) pinnedCount += 1;
      else noNodesCount += 1;

      const limit = Number(c.trafficLimit || 0);
      const highQuota = limit > 0 && Number(c.usedTraffic || 0) / limit >= 0.85;
      const expSoon =
        c.expireAt != null &&
        Number.isFinite(Date.parse(c.expireAt)) &&
        Date.parse(c.expireAt) - now < 7 * 86400 * 1000;
      if (highQuota || expSoon) expiringOrHighQuota += 1;
    }

    const globalTotal = overview?.clients.total ?? totalCount;
    const globalActive = overview?.clients.active ?? clients.filter((c) => !c.banned).length;
    const globalBanned = overview?.clients.banned ?? clients.filter((c) => c.banned).length;
    const dlPct = totalUsed > 0 ? Math.round((totalDl / totalUsed) * 100) : 50;

    return {
      globalTotal,
      globalActive,
      globalBanned,
      totalUsed,
      totalDl,
      totalUl,
      dlPct,
      allNodesCount,
      pinnedCount,
      noNodesCount,
      expiringOrHighQuota,
    };
  }, [clients, overview, totalCount]);

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ['clients'] });
    qc.invalidateQueries({ queryKey: ['dashboard-overview'] });
  };

  const resetMut = useMutation({
    mutationFn: (id: string) => clientsApi.resetTraffic(id),
    onSuccess: () => {
      invalidate();
      toast({ title: isRu ? 'Трафик сброшен' : 'Traffic reset', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка' : 'Failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const banMut = useMutation({
    mutationFn: (id: string) => clientsApi.toggleBan(id),
    onSuccess: () => {
      invalidate();
      toast({ title: isRu ? 'Статус обновлён' : 'Client status updated', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка' : 'Failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => clientsApi.delete(id),
    onSuccess: () => {
      invalidate();
      toast({ title: isRu ? 'Клиент удалён' : 'Client deleted', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка' : 'Failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const bulkMut = useMutation({
    mutationFn: (payload: {
      action: 'ban' | 'unban' | 'reset_traffic' | 'delete' | 'assign_plan';
      planId?: string;
    }) =>
      clientsApi.bulkAction({
        action: payload.action,
        clientIds: selectedIds,
        planId: payload.planId,
      }),
    onSuccess: (out) => {
      setSelectedIds([]);
      invalidate();
      toast({
        title: isRu ? `Обновлено клиентов: ${out.affected}` : `Updated ${out.affected} client(s)`,
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Массовая операция не удалась' : 'Bulk action failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const toggleSelectAll = () => {
    if (selectedIds.length === clients.length) {
      setSelectedIds([]);
    } else {
      setSelectedIds(clients.map((c) => c.id));
    }
  };

  const toggleSelect = (id: string) => {
    setSelectedIds((prev) => (prev.includes(id) ? prev.filter((i) => i !== id) : [...prev, id]));
  };

  const handleCopyUuid = async (c: Client) => {
    try {
      await copyUuidId(c.uuid, c.id);
    } catch {
      toast({
        title: isRu ? 'Не удалось скопировать' : 'Copy failed',
        variant: 'destructive',
      });
    }
  };

  const openEdit = (c: Client, tab: EditClientTab = 'profile') => {
    setEditingTab(tab);
    setEditingClient(c);
  };

  const statusPills = [
    { key: 'all', label: isRu ? 'Все' : 'All', count: clientStats.globalTotal },
    {
      key: 'false',
      label: t('common.active'),
      count: clientStats.globalActive,
      dotClass: 'bg-emerald-400',
    },
    {
      key: 'true',
      label: t('common.banned'),
      count: clientStats.globalBanned,
      dotClass: 'bg-red-500',
    },
  ];

  return (
    <div className="space-y-5">
      <PageHero
        title={t('clients.title')}
        subtitle={t('clients.subtitle')}
        icon={<Users className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs tabular-nums">
            {clientStats.globalActive}/{clientStats.globalTotal} {isRu ? 'активны' : 'active'}
          </Badge>
        }
        actions={
          <>
            <ViewModeToggle value={viewMode} onChange={setViewMode} isRu={isRu} />
            <Button variant="outline" size="sm" onClick={() => setCsvOpen(true)}>
              <Upload className="h-4 w-4" />
              {t('clients.importCsv')}
            </Button>
            <Button variant="outline" size="sm" onClick={() => setMigrateOpen(true)}>
              <ArrowRightLeft className="h-4 w-4" />
              {t('clients.migrate')}
            </Button>
            <Button size="sm" onClick={() => setCreateOpen(true)}>
              <Plus className="h-4 w-4" />
              {t('clients.addClient')}
            </Button>
          </>
        }
      />

      {/* Compact KPI strip */}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded-xl border border-border/80 bg-background/60 px-3 py-2 text-[11px]">
        <span className="inline-flex items-center gap-1 font-medium text-muted-foreground">
          <Users className="h-3.5 w-3.5 text-emerald-400" />
          <span className="font-mono font-bold tabular-nums text-foreground">{clientStats.globalTotal}</span>
          {isRu ? 'клиентов' : 'clients'}
          <span className="text-emerald-500">· {clientStats.globalActive} {isRu ? 'акт.' : 'act.'}</span>
          <span className="text-red-400">· {clientStats.globalBanned} {isRu ? 'блок.' : 'ban.'}</span>
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Activity className="h-3 w-3 text-cyan-400" />
          <span className="text-foreground">{formatBytes(clientStats.totalUsed, { locale: language })}</span>
          <span className="text-emerald-400">↓{formatBytes(clientStats.totalDl, { compact: true, locale: language })}</span>
          <span className="text-cyan-400">↑ {formatBytes(clientStats.totalUl, { compact: true, locale: language })}</span>
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Server className="h-3 w-3 text-primary" />
          <span className="text-foreground">{clientStats.pinnedCount}</span> {isRu ? 'привязаны' : 'pinned'}
          <span>· {clientStats.allNodesCount} {isRu ? 'все ноды' : 'all-nodes'}</span>
          <span>· {clientStats.noNodesCount} {isRu ? 'без нод' : 'none'}</span>
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums">
          <ShieldAlert className="h-3 w-3 text-amber-400" />
          <span className={cn('font-bold', clientStats.expiringOrHighQuota > 0 ? 'text-amber-500' : 'text-muted-foreground')}>
            {clientStats.expiringOrHighQuota}
          </span>
          {isRu ? 'в зоне риска' : 'at risk'}
          <span className="text-muted-foreground">· {plans.length} {isRu ? 'тарифов' : 'plans'}</span>
        </span>
      </div>

      {/* Bulk Action Bar */}
      {selectedIds.length > 0 && (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-primary/35 bg-primary/10 px-4 py-2.5">
          <span className="text-xs font-semibold text-primary">
            {t('clients.bulkSelected', { count: selectedIds.length })}
          </span>
          <div className="flex flex-wrap items-center gap-2">
            <Button size="sm" variant="outline" onClick={() => setBulkNodesOpen(true)}>
              <Server className="h-3.5 w-3.5" /> {t('clients.assignNodes')}
            </Button>
            <Button size="sm" variant="outline" onClick={() => bulkMut.mutate({ action: 'ban' })}>
              <Ban className="h-3.5 w-3.5" /> {t('clients.ban')}
            </Button>
            <Button size="sm" variant="outline" onClick={() => bulkMut.mutate({ action: 'unban' })}>
              <CheckCircle2 className="h-3.5 w-3.5" /> {t('clients.unban')}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => bulkMut.mutate({ action: 'reset_traffic' })}
            >
              <RotateCcw className="h-3.5 w-3.5" /> {t('clients.resetTraffic')}
            </Button>
            <div className="flex items-center gap-1.5">
              <div className="w-40">
                <Select value={bulkPlanId} onValueChange={setBulkPlanId}>
                  <SelectTrigger className="h-8 text-xs">
                    <SelectValue placeholder={t('clients.assignPlan')} />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="none">{t('clients.selectPlan')}</SelectItem>
                    {plans.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <Button
                size="sm"
                variant="secondary"
                disabled={bulkPlanId === 'none'}
                onClick={() => bulkMut.mutate({ action: 'assign_plan', planId: bulkPlanId })}
              >
                {t('clients.applyPlan')}
              </Button>
            </div>
            <Button
              size="sm"
              variant="destructive"
              onClick={() => bulkMut.mutate({ action: 'delete' })}
            >
              <Trash2 className="h-3.5 w-3.5" /> {t('common.delete')}
            </Button>
          </div>
        </div>
      )}

      {/* Filter & Search Toolbar */}
      <Card>
        <CardHeader className="py-2.5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex flex-wrap items-center gap-2">
              {viewMode === 'blades' && clients.length > 0 && (
                <div className="flex items-center gap-1.5 rounded-lg border border-border/70 bg-background/40 px-2.5 py-1.5 text-xs">
                  <Checkbox
                    checked={clients.length > 0 && selectedIds.length === clients.length}
                    onCheckedChange={toggleSelectAll}
                    aria-label="Select all clients"
                  />
                  <span className="text-muted-foreground">{isRu ? 'Выбрать все' : 'Select all'}</span>
                </div>
              )}
              <StatusFilterPills
                items={statusPills}
                value={bannedFilter}
                onChange={(v) => {
                  setBannedFilter(v);
                  setPage(1);
                }}
              />
            </div>

            <div className="flex flex-wrap items-center gap-2.5 flex-1 justify-end min-w-[260px]">
              <div className="relative flex-1 min-w-[200px] max-w-sm">
                <Search className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
                <Input
                  placeholder={t('common.search')}
                  value={search}
                  onChange={(e) => {
                    setSearch(e.target.value);
                    setPage(1);
                  }}
                  className="pl-9"
                />
              </div>
              <div className="w-40">
                <Select
                  value={protocolFilter}
                  onValueChange={(v) => {
                    setProtocolFilter(v);
                    setPage(1);
                  }}
                >
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
              <div className="w-48">
                <Select
                  value={nodeFilter}
                  onValueChange={(v) => {
                    setNodeFilter(v);
                    setPage(1);
                  }}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">{t('clients.allNodesFilter')}</SelectItem>
                    <SelectItem value="__unpinned__">{t('clients.unpinnedOnly')}</SelectItem>
                    <SelectItem value="__pinned__">{t('clients.pinnedOnly')}</SelectItem>
                    <SelectItem value="__no_nodes__">{t('clients.noNodesAssigned')}</SelectItem>
                    {sortedNodes.map((n) => (
                      <SelectItem key={n.id} value={n.id}>
                        {n.name}
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
        <div className="space-y-3">
          <Skeleton className="h-36 w-full rounded-xl" />
          <Skeleton className="h-36 w-full rounded-xl" />
        </div>
      ) : clients.length === 0 ? (
        <RackEmptyState
          icon={Users}
          title={t('common.noData')}
          description={
            isRu
              ? 'Создайте первого клиента или сбросьте активные фильтры поиска.'
              : 'Create your first proxy client or clear your active search filters.'
          }
          action={
            <Button size="sm" onClick={() => setCreateOpen(true)}>
              <Plus className="h-4 w-4" />
              {t('clients.addClient')}
            </Button>
          }
        />
      ) : viewMode === 'blades' ? (
        <div className="grid grid-cols-1 gap-2 2xl:grid-cols-2">
          {clients.map((client) => (
            <ClientBladeCard
              key={client.id}
              client={client}
              selected={selectedIds.includes(client.id)}
              copiedUuid={copiedUuidId === client.id}
              onToggleSelect={() => toggleSelect(client.id)}
              onCopyUuid={() => handleCopyUuid(client)}
              onOpenSubscription={() => setSubClient(client)}
              onEdit={() => openEdit(client, 'profile')}
              onEditNodes={() => openEdit(client, 'nodes')}
              onResetTraffic={() => resetMut.mutate(client.id)}
              onToggleBan={() => banMut.mutate(client.id)}
              onDelete={() => deleteMut.mutate(client.id)}
            />
          ))}
        </div>
      ) : (
        <Card>
          <CardContent className="p-0">
            <div className="overflow-x-auto">
              <table className="data-table w-full min-w-[980px] text-sm">
                <thead className="bg-muted/40">
                  <tr className="border-b border-border/80 text-left text-xs text-muted-foreground">
                    <th className="py-1.5 px-2 w-8">
                      <Checkbox
                        checked={clients.length > 0 && selectedIds.length === clients.length}
                        onCheckedChange={toggleSelectAll}
                        aria-label="Select all clients"
                      />
                    </th>
                    <th className="py-1.5 px-2">{t('clients.colClient')}</th>
                    <th className="py-1.5 px-2">{t('clients.colProtocols')}</th>
                    <th className="py-1.5 px-2">{t('clients.colNodes')}</th>
                    <th className="py-1.5 px-2">{t('clients.colTraffic')}</th>
                    <th className="py-1.5 px-2">{t('clients.colExpires')}</th>
                    <th className="py-1.5 px-2">{t('common.status')}</th>
                    <th className="py-1.5 px-2 text-right">{t('common.actions')}</th>
                  </tr>
                </thead>
                <tbody>
                  {clients.map((client) => (
                    <ClientRow
                      key={client.id}
                      client={client}
                      selected={selectedIds.includes(client.id)}
                      copiedUuid={copiedUuidId === client.id}
                      onToggleSelect={() => toggleSelect(client.id)}
                      onCopyUuid={() => handleCopyUuid(client)}
                      onOpenSubscription={() => setSubClient(client)}
                      onEdit={() => openEdit(client, 'profile')}
                      onEditNodes={() => openEdit(client, 'nodes')}
                      onResetTraffic={() => resetMut.mutate(client.id)}
                      onToggleBan={() => banMut.mutate(client.id)}
                      onDelete={() => deleteMut.mutate(client.id)}
                    />
                  ))}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}

      {totalPages > 1 && (
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          <span>
            {isRu
              ? `Страница ${page} из ${totalPages} (всего: ${totalCount})`
              : `Page ${page} of ${totalPages} (${totalCount} total)`}
          </span>
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="outline"
              disabled={page <= 1}
              onClick={() => setPage((p) => Math.max(1, p - 1))}
            >
              {t('common.prev')}
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={page >= totalPages}
              onClick={() => setPage((p) => p + 1)}
            >
              {t('common.next')}
            </Button>
          </div>
        </div>
      )}

      <CreateClientModal
        open={createOpen}
        onOpenChange={setCreateOpen}
        plans={plans}
        onCreated={invalidate}
      />
      <EditClientModal
        open={Boolean(editingClient)}
        onOpenChange={(open) => !open && setEditingClient(null)}
        client={editingClient}
        plans={plans}
        initialTab={editingTab}
        onSaved={invalidate}
      />
      <SubscriptionModal
        open={Boolean(subClient)}
        onOpenChange={(open) => !open && setSubClient(null)}
        client={subClient}
      />
      <CsvImportModal open={csvOpen} onOpenChange={setCsvOpen} onImported={invalidate} />
      <MigratePanelModal open={migrateOpen} onOpenChange={setMigrateOpen} onMigrated={invalidate} />
      <BulkAssignNodesModal
        open={bulkNodesOpen}
        onOpenChange={setBulkNodesOpen}
        clientIds={selectedIds}
        nodes={nodes}
        onApplied={() => {
          setSelectedIds([]);
          invalidate();
        }}
      />
    </div>
  );
}
