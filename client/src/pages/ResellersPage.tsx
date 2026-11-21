import { useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Activity, BarChart3, Edit, Plus, Trash2, UserCheck, Wallet } from 'lucide-react';
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
import { Switch } from '@/components/ui/switch';
import { PageHero, RackEmptyState } from '@/components/ui/PagePrimitives';
import { MeterBar } from '@/components/nodes/NodeTelemetryPrimitives';
import { getApiErrorMessage, resellersApi } from '@/lib/api';
import { formatBytes, formatDate } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { Reseller } from '@/types';

export function ResellersPage() {
  const qc = useQueryClient();
  const timezone = useUiStore((s) => s.timezone);
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<Reseller | null>(null);
  const [statsReseller, setStatsReseller] = useState<Reseller | null>(null);

  const [name, setName] = useState('');
  const [contactEmail, setContactEmail] = useState('');
  const [contactTelegram, setContactTelegram] = useState('');
  const [maxClients, setMaxClients] = useState('50');
  const [maxNodes, setMaxNodes] = useState('5');
  const [trafficGb, setTrafficGb] = useState('1000');
  const [balance, setBalance] = useState('0');
  const [active, setActive] = useState(true);

  const { data: resellers = [], isLoading } = useQuery({
    queryKey: ['resellers'],
    queryFn: () => resellersApi.list(),
  });

  const { data: stats } = useQuery({
    queryKey: ['reseller-stats', statsReseller?.id],
    queryFn: () => resellersApi.stats(statsReseller!.id),
    enabled: Boolean(statsReseller?.id),
  });

  const summary = useMemo(() => {
    let activeCount = 0;
    let totalBal = 0;
    let totalUsed = 0;
    for (const r of resellers) {
      if (r.active) activeCount += 1;
      totalBal += Number(r.balance ?? 0);
      totalUsed += Number(r.usedTraffic ?? 0);
    }
    return { total: resellers.length, activeCount, totalBal, totalUsed };
  }, [resellers]);

  const openCreate = () => {
    setEditing(null);
    setName('');
    setContactEmail('');
    setContactTelegram('');
    setMaxClients('50');
    setMaxNodes('5');
    setTrafficGb('1000');
    setBalance('0');
    setActive(true);
    setModalOpen(true);
  };

  const openEdit = (r: Reseller) => {
    setEditing(r);
    setName(r.name);
    setContactEmail(r.contactEmail ?? '');
    setContactTelegram(r.contactTelegram ?? '');
    setMaxClients(String(r.maxClients));
    setMaxNodes(String(r.maxNodes));
    setTrafficGb(r.trafficLimit > 0 ? String(Math.round(r.trafficLimit / (1024 * 1024 * 1024))) : '0');
    setBalance(String(r.balance ?? 0));
    setActive(r.active);
    setModalOpen(true);
  };

  const saveMut = useMutation({
    mutationFn: async () => {
      const gb = Number(trafficGb) || 0;
      const payload: Partial<Reseller> = {
        name: name.trim(),
        contactEmail: contactEmail.trim() || null,
        contactTelegram: contactTelegram.trim() || null,
        maxClients: Number(maxClients) || 0,
        maxNodes: Number(maxNodes) || 0,
        trafficLimit: gb > 0 ? Math.round(gb * 1024 * 1024 * 1024) : 0,
        balance: Number(balance) || 0,
        active,
      };
      if (editing) return resellersApi.update(editing.id, payload);
      return resellersApi.create(payload);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['resellers'] });
      setModalOpen(false);
      toast({
        title: editing
          ? isRu
            ? 'Реселлер обновлён'
            : 'Reseller updated'
          : isRu
            ? 'Реселлер создан'
            : 'Reseller created',
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

  const deleteMut = useMutation({
    mutationFn: (id: string) => resellersApi.delete(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['resellers'] });
      toast({ title: isRu ? 'Реселлер удалён' : 'Reseller deleted', variant: 'success' });
    },
  });

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Реселлеры и партнёры' : 'Resellers'}
        subtitle={
          isRu
            ? 'Управление партнёрскими аккаунтами, квотами клиентов, нод и выделенным трафиком'
            : 'Manage reseller tenants, client/node caps, and traffic quotas'
        }
        icon={<UserCheck className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs tabular-nums">
            {summary.activeCount}/{summary.total}
          </Badge>
        }
        actions={
          <Button size="sm" onClick={openCreate}>
            <Plus className="h-4 w-4" />
            <span>{isRu ? 'Добавить реселлера' : 'Create Reseller'}</span>
          </Button>
        }
      />

      {/* 3-Card KPI Ribbon */}
      <div className="grid gap-4 sm:grid-cols-3">
        <Card className="p-4 space-y-1.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Активные реселлеры' : 'Active Resellers'}</span>
            <UserCheck className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {summary.activeCount} / {summary.total}
          </div>
        </Card>

        <Card className="p-4 space-y-1.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Суммарный баланс' : 'Total Balance'}</span>
            <Wallet className="h-4 w-4 text-cyan-400" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">${summary.totalBal.toFixed(2)}</div>
        </Card>

        <Card className="p-4 space-y-1.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Использованный трафик' : 'Aggregate Traffic'}</span>
            <Activity className="h-4 w-4 text-primary" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {formatBytes(summary.totalUsed, { locale: language })}
          </div>
        </Card>
      </div>

      {isLoading ? (
        <Card className="p-8 text-center text-sm text-muted-foreground">
          {isRu ? 'Загрузка реселлеров...' : 'Loading resellers...'}
        </Card>
      ) : resellers.length === 0 ? (
        <RackEmptyState
          icon={UserCheck}
          title={isRu ? 'Реселлеры ещё не добавлены' : 'No resellers found'}
          description={
            isRu
              ? 'Создайте партнёрский аккаунт с выделенными лимитами клиентов, нод и трафика.'
              : 'Create a reseller account with dedicated client, node, and bandwidth quotas.'
          }
          action={
            <Button size="sm" onClick={openCreate}>
              <Plus className="h-4 w-4" />
              <span>{isRu ? 'Добавить реселлера' : 'Create Reseller'}</span>
            </Button>
          }
        />
      ) : (
        <Card>
          <CardContent className="p-0">
            <div className="overflow-x-auto">
              <table className="data-table w-full text-sm">
                <thead className="bg-muted/40">
                  <tr className="border-b text-left text-xs text-muted-foreground">
                    <th className="py-2.5 px-3">{isRu ? 'Партнёр' : 'Name'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Лимиты (Клиенты / Ноды)' : 'Caps (Clients / Nodes)'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Квота трафика' : 'Traffic Usage'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Баланс' : 'Balance'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Статус' : 'Status'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Истекает' : 'Expires'}</th>
                    <th className="py-2.5 px-3 text-right">{isRu ? 'Действия' : 'Actions'}</th>
                  </tr>
                </thead>
                <tbody>
                  {resellers.map((r) => {
                    const pct =
                      r.trafficLimit > 0
                        ? Math.min(100, Math.round((r.usedTraffic / r.trafficLimit) * 100))
                        : 0;
                    return (
                      <tr key={r.id} className="border-b last:border-b-0 hover:bg-accent/30">
                        <td className="py-2.5 px-3 font-medium">
                          <div className="flex items-center gap-2">
                            <UserCheck className="h-4 w-4 text-primary" />
                            <div>
                              <div>{r.name}</div>
                              <div className="text-[11px] text-muted-foreground">
                                {r.contactEmail || r.contactTelegram || '—'}
                              </div>
                            </div>
                          </div>
                        </td>
                        <td className="py-2.5 px-3 font-mono text-xs">
                          {r.maxClients} {isRu ? 'кл.' : 'clients'} · {r.maxNodes} {isRu ? 'нод' : 'nodes'}
                        </td>
                        <td className="py-2.5 px-3 text-xs min-w-[160px] space-y-1">
                          <div className="font-mono tabular-nums">
                            {formatBytes(r.usedTraffic, { locale: language })} /{' '}
                            {r.trafficLimit > 0 ? formatBytes(r.trafficLimit, { locale: language }) : '∞'}
                          </div>
                          {r.trafficLimit > 0 && <MeterBar value={pct} tone="emerald" className="h-1.5" />}
                        </td>
                        <td className="py-2.5 px-3 font-mono text-xs">${Number(r.balance ?? 0).toFixed(2)}</td>
                        <td className="py-2.5 px-3">
                          <Badge variant={r.active ? 'success' : 'secondary'}>
                            {r.active ? (isRu ? 'Активен' : 'Active') : isRu ? 'Отключён' : 'Inactive'}
                          </Badge>
                        </td>
                        <td className="py-2.5 px-3 text-xs">
                          {r.expiresAt ? formatDate(r.expiresAt, undefined, timezone) : isRu ? 'Никогда' : 'Never'}
                        </td>
                        <td className="py-2.5 px-3 text-right">
                          <div className="inline-flex gap-1">
                            <Button size="icon" variant="ghost" onClick={() => setStatsReseller(r)}>
                              <BarChart3 className="h-4 w-4" />
                            </Button>
                            <Button size="icon" variant="ghost" onClick={() => openEdit(r)}>
                              <Edit className="h-4 w-4" />
                            </Button>
                            <Button size="icon" variant="ghost" onClick={() => deleteMut.mutate(r.id)}>
                              <Trash2 className="h-4 w-4 text-red-400" />
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

      <Dialog open={modalOpen} onOpenChange={setModalOpen}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>
              {editing
                ? isRu
                  ? `Редактировать реселлера: ${editing.name}`
                  : `Edit Reseller: ${editing.name}`
                : isRu
                  ? 'Добавить реселлера'
                  : 'Create Reseller'}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Имя / Название' : 'Name'}</Label>
                <Input value={name} onChange={(e) => setName(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>Email</Label>
                <Input value={contactEmail} onChange={(e) => setContactEmail(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>Telegram</Label>
                <Input value={contactTelegram} onChange={(e) => setContactTelegram(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Баланс ($)' : 'Balance ($)'}</Label>
                <Input type="number" step="0.01" value={balance} onChange={(e) => setBalance(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Макс. клиентов' : 'Max Clients'}</Label>
                <Input type="number" min={0} value={maxClients} onChange={(e) => setMaxClients(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Макс. нод' : 'Max Nodes'}</Label>
                <Input type="number" min={0} value={maxNodes} onChange={(e) => setMaxNodes(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Лимит трафика (ГБ, 0 = безлимит)' : 'Traffic Limit (GB, 0 = unlimited)'}</Label>
                <Input type="number" min={0} value={trafficGb} onChange={(e) => setTrafficGb(e.target.value)} />
              </div>
              <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                <Label>{isRu ? 'Активен' : 'Active'}</Label>
                <Switch checked={active} onCheckedChange={setActive} />
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setModalOpen(false)}>
              {isRu ? 'Отмена' : 'Cancel'}
            </Button>
            <Button
              type="button"
              onClick={() => saveMut.mutate()}
              loading={saveMut.isPending}
              disabled={!name.trim()}
            >
              {isRu ? 'Сохранить' : 'Save Reseller'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={Boolean(statsReseller)} onOpenChange={(o) => !o && setStatsReseller(null)}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>
              {isRu ? `Квота реселлера: ${statsReseller?.name}` : `Reseller Quota: ${statsReseller?.name}`}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-3 text-xs">
            <div className="flex justify-between rounded-lg border p-3">
              <span className="text-muted-foreground">{isRu ? 'Активные клиенты' : 'Active Clients'}</span>
              <span className="font-mono font-semibold">
                {stats?.clients.active ?? 0} / {statsReseller?.maxClients ?? 0}
              </span>
            </div>
            <div className="flex justify-between rounded-lg border p-3">
              <span className="text-muted-foreground">{isRu ? 'Использовано трафика' : 'Traffic Used'}</span>
              <span className="font-mono font-semibold">
                {formatBytes(stats?.usedTraffic ?? statsReseller?.usedTraffic, { locale: language })} /{' '}
                {statsReseller?.trafficLimit
                  ? formatBytes(statsReseller.trafficLimit, { locale: language })
                  : '∞'}
              </span>
            </div>
          </div>
          <DialogFooter>
            <Button onClick={() => setStatsReseller(null)}>{isRu ? 'Закрыть' : 'Close'}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
