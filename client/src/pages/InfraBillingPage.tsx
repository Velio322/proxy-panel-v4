import { useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  AlertCircle,
  Building2,
  CalendarClock,
  DollarSign,
  Edit,
  Plus,
  Receipt,
  Server,
  Trash2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Skeleton } from '@/components/ui/skeleton';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { PageHero, RackEmptyState } from '@/components/ui/PagePrimitives';
import { getApiErrorMessage, infraBillingApi, nodesApi } from '@/lib/api';
import { formatDate } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { InfraNodeBilling, InfraProvider } from '@/types';

const BILLING_CYCLES: Array<InfraNodeBilling['billingCycle']> = ['MONTHLY', 'QUARTERLY', 'YEARLY'];
const CURRENCIES = ['USD', 'EUR', 'RUB', 'GBP'] as const;

export function normalizeMonthlyAmount(amount: number, cycle: InfraNodeBilling['billingCycle']): number {
  if (!Number.isFinite(amount) || amount <= 0) return 0;
  if (cycle === 'QUARTERLY') return amount / 3;
  if (cycle === 'YEARLY') return amount / 12;
  return amount;
}

export function InfraBillingPage() {
  const qc = useQueryClient();
  const timezone = useUiStore((s) => s.timezone);
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  // Provider modal state
  const [providerModalOpen, setProviderModalOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<InfraProvider | null>(null);
  const [provName, setProvName] = useState('');
  const [provWebsite, setProvWebsite] = useState('');
  const [provSupportUrl, setProvSupportUrl] = useState('');
  const [provNotes, setProvNotes] = useState('');

  // Node billing modal state
  const [billingModalOpen, setBillingModalOpen] = useState(false);
  const [editingBilling, setEditingBilling] = useState<InfraNodeBilling | null>(null);
  const [billNodeId, setBillNodeId] = useState('');
  const [billProviderId, setBillProviderId] = useState('');
  const [billAmount, setBillAmount] = useState('10');
  const [billCurrency, setBillCurrency] = useState('USD');
  const [billCycle, setBillCycle] = useState<InfraNodeBilling['billingCycle']>('MONTHLY');
  const [billNextDue, setBillNextDue] = useState('');
  const [billAutoRenew, setBillAutoRenew] = useState(true);
  const [billNotes, setBillNotes] = useState('');

  const { data: providers = [], isLoading: providersLoading } = useQuery({
    queryKey: ['infra-providers'],
    queryFn: () => infraBillingApi.listProviders(),
  });

  const { data: billings = [], isLoading: billingsLoading } = useQuery({
    queryKey: ['infra-node-billings'],
    queryFn: () => infraBillingApi.listNodeBillings(),
  });

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
  });

  const monthlyTotalsByCurrency = useMemo(() => {
    const totals: Record<string, number> = {};
    for (const b of billings) {
      const cur = b.currency || 'USD';
      totals[cur] = (totals[cur] || 0) + normalizeMonthlyAmount(Number(b.amount) || 0, b.billingCycle);
    }
    return totals;
  }, [billings]);

  const upcomingRenewals = useMemo(() => {
    const now = Date.now();
    const sevenDaysMs = 7 * 24 * 3600 * 1000;
    return billings.filter((b) => {
      if (!b.nextDueDate) return false;
      const due = new Date(b.nextDueDate).getTime();
      return !Number.isNaN(due) && due - now <= sevenDaysMs;
    });
  }, [billings]);

  const openCreateProvider = () => {
    setEditingProvider(null);
    setProvName('');
    setProvWebsite('');
    setProvSupportUrl('');
    setProvNotes('');
    setProviderModalOpen(true);
  };

  const openEditProvider = (p: InfraProvider) => {
    setEditingProvider(p);
    setProvName(p.name);
    setProvWebsite(p.website ?? '');
    setProvSupportUrl(p.supportUrl ?? '');
    setProvNotes(p.notes ?? '');
    setProviderModalOpen(true);
  };

  const saveProviderMut = useMutation({
    mutationFn: async () => {
      const payload: Partial<InfraProvider> = {
        name: provName.trim(),
        website: provWebsite.trim() || null,
        supportUrl: provSupportUrl.trim() || null,
        notes: provNotes.trim() || null,
      };
      if (editingProvider) return infraBillingApi.updateProvider(editingProvider.id, payload);
      return infraBillingApi.createProvider(payload);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['infra-providers'] });
      setProviderModalOpen(false);
      toast({
        title: editingProvider
          ? isRu
            ? 'Провайдер обновлён'
            : 'Provider updated'
          : isRu
            ? 'Провайдер добавлен'
            : 'Provider added',
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка сохранения' : 'Save failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const deleteProviderMut = useMutation({
    mutationFn: (id: string) => infraBillingApi.deleteProvider(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['infra-providers'] }),
  });

  const openCreateBilling = () => {
    setEditingBilling(null);
    setBillNodeId(nodes[0]?.id || '');
    setBillProviderId(providers[0]?.id || '');
    setBillAmount('10');
    setBillCurrency('USD');
    setBillCycle('MONTHLY');
    const nextMonth = new Date(Date.now() + 30 * 86400_000).toISOString().slice(0, 10);
    setBillNextDue(nextMonth);
    setBillAutoRenew(true);
    setBillNotes('');
    setBillingModalOpen(true);
  };

  const openEditBilling = (b: InfraNodeBilling) => {
    setEditingBilling(b);
    setBillNodeId(b.nodeId);
    setBillProviderId(b.providerId);
    setBillAmount(String(b.amount));
    setBillCurrency(b.currency || 'USD');
    setBillCycle(b.billingCycle);
    setBillNextDue((b.nextDueDate || '').slice(0, 10));
    setBillAutoRenew(b.autoRenew);
    setBillNotes(b.notes ?? '');
    setBillingModalOpen(true);
  };

  const saveBillingMut = useMutation({
    mutationFn: async () => {
      const payload: Partial<InfraNodeBilling> = {
        nodeId: billNodeId,
        providerId: billProviderId,
        amount: Number(billAmount) || 0,
        currency: billCurrency,
        billingCycle: billCycle,
        nextDueDate: billNextDue,
        autoRenew: billAutoRenew,
        notes: billNotes.trim() || null,
      };
      if (editingBilling) return infraBillingApi.updateNodeBilling(editingBilling.id, payload);
      return infraBillingApi.createNodeBilling(payload);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['infra-node-billings'] });
      setBillingModalOpen(false);
      toast({
        title: editingBilling
          ? isRu
            ? 'Запись оплаты обновлена'
            : 'Billing record updated'
          : isRu
            ? 'Запись оплаты создана'
            : 'Billing record created',
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка сохранения' : 'Save failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const deleteBillingMut = useMutation({
    mutationFn: (id: string) => infraBillingApi.deleteNodeBilling(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['infra-node-billings'] }),
  });

  const formattedMonthlySummary = useMemo(() => {
    const entries = Object.entries(monthlyTotalsByCurrency);
    if (entries.length === 0) return isRu ? '$0.00 / мес' : '$0.00 / mo';
    return (
      entries.map(([cur, val]) => `${val.toFixed(2)} ${cur}`).join(' + ') +
      (isRu ? ' / мес' : ' / mo')
    );
  }, [monthlyTotalsByCurrency, isRu]);

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Биллинг инфраструктуры' : 'Infrastructure Billing'}
        subtitle={
          isRu
            ? 'Учёт хостинг-провайдеров, дат продления VPS-серверов и нормализованных ежемесячных расходов'
            : 'Track hosting providers, server renewal dates, and normalized monthly infrastructure costs.'
        }
        icon={<Receipt className="h-4 w-4" />}
        actions={
          <>
            <Button variant="outline" size="sm" onClick={openCreateProvider}>
              <Plus className="h-4 w-4" />
              <span>{isRu ? 'Добавить провайдера' : 'Add Provider'}</span>
            </Button>
            <Button size="sm" onClick={openCreateBilling}>
              <Plus className="h-4 w-4" />
              <span>{isRu ? 'Добавить оплату сервера' : 'Add Server Billing'}</span>
            </Button>
          </>
        }
      />

      {/* 4-Card KPI Summary Ribbon */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Card className="p-4 space-y-1.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Расходы в месяц' : 'Monthly Spend (Normalized)'}</span>
            <DollarSign className="h-4 w-4 text-primary" />
          </div>
          <div className="font-mono text-xl font-bold tabular-nums" data-testid="monthly-spend-summary">
            {formattedMonthlySummary}
          </div>
        </Card>

        <Card className="p-4 space-y-1.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Продление ≤ 7 дней' : 'Upcoming Renewals (≤ 7d)'}</span>
            <CalendarClock className="h-4 w-4 text-amber-400" />
          </div>
          <div className="font-mono text-xl font-bold tabular-nums" data-testid="upcoming-renewals-count">
            {upcomingRenewals.length}
          </div>
        </Card>

        <Card className="p-4 space-y-1.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Провайдеры' : 'Active Providers'}</span>
            <Building2 className="h-4 w-4 text-sky-400" />
          </div>
          <div className="font-mono text-xl font-bold tabular-nums">{providers.length}</div>
        </Card>

        <Card className="p-4 space-y-1.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Серверов на учёте' : 'Tracked Servers'}</span>
            <Server className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="font-mono text-xl font-bold tabular-nums">{billings.length}</div>
        </Card>
      </div>

      {upcomingRenewals.length > 0 && (
        <Card className="border-amber-500/40 bg-amber-500/5">
          <CardContent className="flex flex-wrap items-center justify-between gap-2 py-3 text-xs">
            <div className="flex items-center gap-2 text-amber-300">
              <AlertCircle className="h-4 w-4 shrink-0" />
              <span>
                {isRu
                  ? `Требуется продление в течение 7 дней (${upcomingRenewals.length}): `
                  : `${upcomingRenewals.length} server(s) due within 7 days: `}
                {upcomingRenewals
                  .map(
                    (r) =>
                      `${r.nodeName || r.nodeId} (${r.amount} ${r.currency} · ${formatDate(r.nextDueDate, undefined, timezone)})`,
                  )
                  .join(', ')}
              </span>
            </div>
          </CardContent>
        </Card>
      )}

      <Tabs defaultValue="servers">
        <TabsList>
          <TabsTrigger value="servers">
            {isRu ? `Оплата серверов (${billings.length})` : `Server Billing (${billings.length})`}
          </TabsTrigger>
          <TabsTrigger value="providers">
            {isRu ? `Провайдеры (${providers.length})` : `Providers (${providers.length})`}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="servers">
          {billingsLoading ? (
            <Card className="p-4 space-y-2">
              <Skeleton className="h-9 w-full" />
              <Skeleton className="h-9 w-full" />
            </Card>
          ) : billings.length === 0 ? (
            <RackEmptyState
              icon={Server}
              title={isRu ? 'Записи об оплате серверов отсутствуют' : 'No server billing records configured'}
              description={
                isRu
                  ? 'Добавьте график оплаты серверов для отслеживания ежемесячных затрат и сроков продления.'
                  : 'Add server billing schedules to track monthly costs and upcoming renewal dates.'
              }
              action={
                <Button size="sm" onClick={openCreateBilling}>
                  <Plus className="h-4 w-4" />
                  <span>{isRu ? 'Добавить оплату сервера' : 'Add Server Billing'}</span>
                </Button>
              }
            />
          ) : (
            <Card>
              <CardContent className="p-0">
                <div className="overflow-x-auto">
                  <table className="data-table w-full text-xs">
                    <thead className="bg-muted/40">
                      <tr className="border-b text-left text-muted-foreground">
                        <th className="py-2.5 px-3">{isRu ? 'Нода' : 'Node'}</th>
                        <th className="py-2.5 px-3">{isRu ? 'Провайдер' : 'Provider'}</th>
                        <th className="py-2.5 px-3">{isRu ? 'Сумма / Период' : 'Amount / Cycle'}</th>
                        <th className="py-2.5 px-3">{isRu ? 'В месяц' : 'Monthly Equivalent'}</th>
                        <th className="py-2.5 px-3">{isRu ? 'Следующая оплата' : 'Next Due Date'}</th>
                        <th className="py-2.5 px-3">{isRu ? 'Автопродление' : 'Auto-Renew'}</th>
                        <th className="py-2.5 px-3 text-right">{isRu ? 'Действия' : 'Actions'}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {billings.map((b) => {
                        const nodeName =
                          b.nodeName || nodes.find((n) => n.id === b.nodeId)?.name || b.nodeId;
                        const providerName =
                          b.providerName ||
                          providers.find((p) => p.id === b.providerId)?.name ||
                          b.providerId;
                        const monthlyEq = normalizeMonthlyAmount(Number(b.amount) || 0, b.billingCycle);
                        return (
                          <tr key={b.id} className="border-b last:border-b-0 hover:bg-muted/20">
                            <td className="py-2.5 px-3 font-semibold">{nodeName}</td>
                            <td className="py-2.5 px-3">{providerName}</td>
                            <td className="py-2.5 px-3 font-mono">
                              {b.amount} {b.currency} ({b.billingCycle})
                            </td>
                            <td className="py-2.5 px-3 font-mono">
                              {monthlyEq.toFixed(2)} {b.currency}/{isRu ? 'мес' : 'mo'}
                            </td>
                            <td className="py-2.5 px-3">{formatDate(b.nextDueDate, undefined, timezone)}</td>
                            <td className="py-2.5 px-3">
                              <Badge variant={b.autoRenew ? 'success' : 'secondary'}>
                                {b.autoRenew ? (isRu ? 'Да' : 'Yes') : isRu ? 'Вручную' : 'Manual'}
                              </Badge>
                            </td>
                            <td className="py-2.5 px-3 text-right">
                              <div className="flex justify-end gap-1">
                                <Button size="icon" variant="ghost" onClick={() => openEditBilling(b)}>
                                  <Edit className="h-3.5 w-3.5" />
                                </Button>
                                <Button
                                  size="icon"
                                  variant="ghost"
                                  onClick={() => deleteBillingMut.mutate(b.id)}
                                >
                                  <Trash2 className="h-3.5 w-3.5 text-red-400" />
                                </Button>
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
        </TabsContent>

        <TabsContent value="providers">
          {providersLoading ? (
            <Card className="p-8 text-center text-sm text-muted-foreground">
              {isRu ? 'Загрузка провайдеров...' : 'Loading providers...'}
            </Card>
          ) : providers.length === 0 ? (
            <RackEmptyState
              icon={Building2}
              title={isRu ? 'Хостинг-провайдеры не добавлены' : 'No infrastructure providers configured'}
              description={
                isRu
                  ? 'Добавьте хостинг-провайдера (Hetzner, OVH, Vultr и др.) для привязки серверов.'
                  : 'Add a hosting provider (Hetzner, OVH, Vultr, etc.) to link your server nodes.'
              }
              action={
                <Button size="sm" onClick={openCreateProvider}>
                  <Plus className="h-4 w-4" />
                  <span>{isRu ? 'Добавить провайдера' : 'Add Provider'}</span>
                </Button>
              }
            />
          ) : (
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {providers.map((p) => (
                <div
                  key={p.id}
                  className="node-blade relative overflow-hidden rounded-xl border border-border/80 p-4 pl-5 space-y-2"
                >
                  <div className="absolute inset-y-0 left-0 w-1 bg-cyan-400" aria-hidden="true" />
                  <div className="flex items-center justify-between">
                    <span className="font-bold text-sm">{p.name}</span>
                    <div className="flex gap-1">
                      <Button size="icon" variant="ghost" onClick={() => openEditProvider(p)}>
                        <Edit className="h-3.5 w-3.5" />
                      </Button>
                      <Button
                        size="icon"
                        variant="ghost"
                        onClick={() => deleteProviderMut.mutate(p.id)}
                      >
                        <Trash2 className="h-3.5 w-3.5 text-red-400" />
                      </Button>
                    </div>
                  </div>
                  <div className="space-y-1 text-xs text-muted-foreground">
                    {p.website && <div>Website: {p.website}</div>}
                    {p.supportUrl && <div>Support: {p.supportUrl}</div>}
                    {p.notes && <div>{p.notes}</div>}
                  </div>
                </div>
              ))}
            </div>
          )}
        </TabsContent>
      </Tabs>

      {/* Provider Modal */}
      <Dialog open={providerModalOpen} onOpenChange={setProviderModalOpen}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>
              {editingProvider
                ? isRu
                  ? 'Редактировать провайдера'
                  : 'Edit Provider'
                : isRu
                  ? 'Добавить хостинг-провайдера'
                  : 'Add Hosting Provider'}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label>{isRu ? 'Название провайдера' : 'Provider Name'}</Label>
              <Input
                value={provName}
                onChange={(e) => setProvName(e.target.value)}
                placeholder="Hetzner / OVH / Vultr"
              />
            </div>
            <div className="space-y-1.5">
              <Label>Website URL</Label>
              <Input
                value={provWebsite}
                onChange={(e) => setProvWebsite(e.target.value)}
                placeholder="https://hetzner.com"
              />
            </div>
            <div className="space-y-1.5">
              <Label>{isRu ? 'Панель управления / Поддержка' : 'Support / Billing Portal URL'}</Label>
              <Input
                value={provSupportUrl}
                onChange={(e) => setProvSupportUrl(e.target.value)}
                placeholder="https://console.hetzner.cloud"
              />
            </div>
            <div className="space-y-1.5">
              <Label>{isRu ? 'Заметки' : 'Notes'}</Label>
              <Input value={provNotes} onChange={(e) => setProvNotes(e.target.value)} />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setProviderModalOpen(false)}>
              {isRu ? 'Отмена' : 'Cancel'}
            </Button>
            <Button
              onClick={() => saveProviderMut.mutate()}
              loading={saveProviderMut.isPending}
              disabled={!provName.trim()}
            >
              {isRu ? 'Сохранить' : 'Save Provider'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Server Billing Modal */}
      <Dialog open={billingModalOpen} onOpenChange={setBillingModalOpen}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>
              {editingBilling
                ? isRu
                  ? 'Редактировать запись оплаты'
                  : 'Edit Server Billing'
                : isRu
                  ? 'Добавить оплату сервера'
                  : 'Add Server Billing Record'}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Нода' : 'Node'}</Label>
                {nodes.length > 0 ? (
                  <Select value={billNodeId} onValueChange={setBillNodeId}>
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {nodes.map((n) => (
                        <SelectItem key={n.id} value={n.id}>
                          {n.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                ) : (
                  <Input
                    value={billNodeId}
                    onChange={(e) => setBillNodeId(e.target.value)}
                    placeholder="Node ID"
                  />
                )}
              </div>

              <div className="space-y-1.5">
                <Label>{isRu ? 'Провайдер' : 'Provider'}</Label>
                {providers.length > 0 ? (
                  <Select value={billProviderId} onValueChange={setBillProviderId}>
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {providers.map((p) => (
                        <SelectItem key={p.id} value={p.id}>
                          {p.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                ) : (
                  <Input
                    value={billProviderId}
                    onChange={(e) => setBillProviderId(e.target.value)}
                    placeholder="Provider ID"
                  />
                )}
              </div>

              <div className="space-y-1.5">
                <Label>{isRu ? 'Сумма' : 'Amount'}</Label>
                <Input
                  type="number"
                  min={0}
                  step="0.01"
                  value={billAmount}
                  onChange={(e) => setBillAmount(e.target.value)}
                />
              </div>

              <div className="space-y-1.5">
                <Label>{isRu ? 'Валюта' : 'Currency'}</Label>
                <Select value={billCurrency} onValueChange={setBillCurrency}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {CURRENCIES.map((c) => (
                      <SelectItem key={c} value={c}>
                        {c}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-1.5">
                <Label>{isRu ? 'Цикл оплаты' : 'Billing Cycle'}</Label>
                <Select
                  value={billCycle}
                  onValueChange={(v) => setBillCycle(v as InfraNodeBilling['billingCycle'])}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {BILLING_CYCLES.map((c) => (
                      <SelectItem key={c} value={c}>
                        {c}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-1.5">
                <Label>{isRu ? 'Следующая дата оплаты' : 'Next Due Date'}</Label>
                <Input
                  type="date"
                  value={billNextDue}
                  onChange={(e) => setBillNextDue(e.target.value)}
                />
              </div>
            </div>

            <div className="flex items-center justify-between rounded-lg border px-3 py-2">
              <Label>{isRu ? 'Автопродление включено' : 'Auto-Renew Enabled'}</Label>
              <Switch checked={billAutoRenew} onCheckedChange={setBillAutoRenew} />
            </div>

            <div className="space-y-1.5">
              <Label>{isRu ? 'Заметки' : 'Notes'}</Label>
              <Input value={billNotes} onChange={(e) => setBillNotes(e.target.value)} />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setBillingModalOpen(false)}>
              {isRu ? 'Отмена' : 'Cancel'}
            </Button>
            <Button
              onClick={() => saveBillingMut.mutate()}
              loading={saveBillingMut.isPending}
              disabled={!billNodeId || !billProviderId}
            >
              {isRu ? 'Сохранить запись' : 'Save Billing Record'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
