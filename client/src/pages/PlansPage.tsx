import { useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Edit, Plus, Smartphone, Sparkles, Trash2, Users } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
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
import { Switch } from '@/components/ui/switch';
import { Checkbox } from '@/components/ui/checkbox';
import { PageHero, RackEmptyState } from '@/components/ui/PagePrimitives';
import { getApiErrorMessage, plansApi } from '@/lib/api';
import { PROTOCOL_COLORS, PROTOCOLS, cn, formatBytes } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { Plan, PlanType, Protocol, TrafficResetStrategy } from '@/types';

const RESET_STRATEGIES: TrafficResetStrategy[] = [
  'NO_RESET',
  'DAY',
  'WEEK',
  'MONTH',
  'MONTH_ROLLING',
  'ON_HOLD',
];

export function PlansPage() {
  const qc = useQueryClient();
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<Plan | null>(null);

  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [type, setType] = useState<PlanType>('USER');
  const [price, setPrice] = useState('0');
  const [currency, setCurrency] = useState('USD');
  const [duration, setDuration] = useState('30');
  const [trafficGb, setTrafficGb] = useState('100');
  const [maxClients, setMaxClients] = useState('1');
  const [hwidLimit, setHwidLimit] = useState('3');
  const [maxSpeed, setMaxSpeed] = useState('0');
  const [defaultResetStrategy, setDefaultResetStrategy] = useState<TrafficResetStrategy>('NO_RESET');
  const [protocols, setProtocols] = useState<Protocol[]>(['VLESS']);
  const [active, setActive] = useState(true);

  const { data: plans = [], isLoading } = useQuery({
    queryKey: ['plans'],
    queryFn: () => plansApi.list(),
  });

  const stats = useMemo(() => {
    const activeCount = plans.filter((p) => p.active).length;
    const userPlans = plans.filter((p) => p.type === 'USER').length;
    const resellerPlans = plans.filter((p) => p.type === 'RESELLER').length;
    return {
      total: plans.length,
      activeCount,
      userPlans,
      resellerPlans,
    };
  }, [plans]);

  const openCreate = () => {
    setEditing(null);
    setName('');
    setDescription('');
    setType('USER');
    setPrice('0');
    setCurrency('USD');
    setDuration('30');
    setTrafficGb('100');
    setMaxClients('1');
    setHwidLimit('3');
    setMaxSpeed('0');
    setDefaultResetStrategy('NO_RESET');
    setProtocols(['VLESS']);
    setActive(true);
    setModalOpen(true);
  };

  const openEdit = (plan: Plan) => {
    setEditing(plan);
    setName(plan.name);
    setDescription(plan.description ?? '');
    setType(plan.type);
    setPrice(String(plan.price ?? 0));
    setCurrency(plan.currency || 'USD');
    setDuration(String(plan.duration ?? plan.periodDays ?? 30));
    const bytes = plan.trafficLimit ?? plan.trafficLimitBytes ?? 0;
    setTrafficGb(bytes > 0 ? String(Math.round(bytes / (1024 * 1024 * 1024))) : '0');
    setMaxClients(String(plan.maxClients ?? plan.deviceLimit ?? 1));
    setHwidLimit(plan.hwidLimit !== undefined && plan.hwidLimit !== null ? String(plan.hwidLimit) : '');
    setMaxSpeed(String(plan.maxSpeed ?? plan.speedLimitMbps ?? 0));
    setDefaultResetStrategy(plan.defaultResetStrategy ?? 'NO_RESET');
    setProtocols(plan.protocols?.length ? plan.protocols : ['VLESS']);
    setActive(plan.active);
    setModalOpen(true);
  };

  const saveMut = useMutation({
    mutationFn: async () => {
      const gb = Number(trafficGb) || 0;
      const bytes = gb > 0 ? Math.round(gb * 1024 * 1024 * 1024) : 0;
      const dur = Number(duration) || 30;
      const mc = Number(maxClients) || 1;
      const spd = Number(maxSpeed) || 0;
      const payload: Partial<Plan> = {
        name: name.trim(),
        description: description.trim() || null,
        type,
        price: Number(price) || 0,
        currency: currency.trim() || 'USD',
        duration: dur,
        periodDays: dur,
        trafficLimit: bytes,
        trafficLimitBytes: bytes,
        maxClients: mc,
        deviceLimit: mc,
        hwidLimit: hwidLimit.trim() !== '' ? Number(hwidLimit) : null,
        maxSpeed: spd,
        speedLimitMbps: spd,
        defaultResetStrategy,
        protocols,
        active,
      };
      if (editing) {
        return plansApi.update(editing.id, payload);
      }
      return plansApi.create(payload);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['plans'] });
      setModalOpen(false);
      toast({
        title: editing
          ? isRu
            ? 'Тариф обновлён'
            : 'Plan updated'
          : isRu
            ? 'Тариф создан'
            : 'Plan created',
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
    mutationFn: (id: string) => plansApi.delete(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['plans'] });
      toast({ title: isRu ? 'Тариф удалён' : 'Plan deleted', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка удаления' : 'Delete failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Тарифные планы' : 'Subscription Plans'}
        subtitle={
          isRu
            ? 'Настройка квот трафика, лимитов HWID-устройств, стратегий сброса и разрешённых протоколов'
            : 'Define traffic quotas, HWID device limits, reset strategies, and supported protocols'
        }
        icon={<Sparkles className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs tabular-nums">
            {stats.activeCount}/{stats.total} {isRu ? 'активны' : 'active'}
          </Badge>
        }
        actions={
          <Button size="sm" onClick={openCreate}>
            <Plus className="h-4 w-4" />
            <span>{isRu ? 'Создать тариф' : 'Create Plan'}</span>
          </Button>
        }
      />

      {/* 3-Card Plans KPI Ribbon */}
      <div className="grid gap-4 sm:grid-cols-3">
        <Card className="p-4 space-y-1.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Всего тарифов' : 'Total Plans'}</span>
            <Sparkles className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">{stats.total}</div>
          <div className="text-[11px] text-muted-foreground">
            {stats.activeCount} {isRu ? 'активных' : 'active'}
          </div>
        </Card>

        <Card className="p-4 space-y-1.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Клиентские тарифы (USER)' : 'User Plans'}</span>
            <Users className="h-4 w-4 text-primary" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">{stats.userPlans}</div>
          <div className="text-[11px] text-muted-foreground">
            {isRu ? 'Шаблоны для конечных пользователей' : 'End-user subscription templates'}
          </div>
        </Card>

        <Card className="p-4 space-y-1.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Тарифы реселлеров' : 'Reseller Plans'}</span>
            <Smartphone className="h-4 w-4 text-cyan-400" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">{stats.resellerPlans}</div>
          <div className="text-[11px] text-muted-foreground">
            {isRu ? 'Пакеты для партнёров' : 'Partner bulk allocation tiers'}
          </div>
        </Card>
      </div>

      {isLoading ? (
        <div className="py-10 text-center text-sm text-muted-foreground">
          {isRu ? 'Загрузка тарифов...' : 'Loading plans...'}
        </div>
      ) : plans.length === 0 ? (
        <RackEmptyState
          icon={Sparkles}
          title={isRu ? 'Тарифные планы ещё не созданы' : 'No plans created yet'}
          description={
            isRu
              ? 'Создайте тарифный план для быстрого назначения квот трафика, лимитов устройств и протоколов клиентам.'
              : 'Create a subscription plan to standardize traffic limits, HWID caps, and allowed protocols.'
          }
          action={
            <Button size="sm" onClick={openCreate}>
              <Plus className="h-4 w-4" />
              <span>{isRu ? 'Создать тариф' : 'Create Plan'}</span>
            </Button>
          }
        />
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {plans.map((plan) => {
            const bytes = plan.trafficLimit ?? plan.trafficLimitBytes ?? 0;
            const dur = plan.duration ?? plan.periodDays ?? 30;
            return (
              <div
                key={plan.id}
                className="node-blade relative flex flex-col justify-between overflow-hidden rounded-xl border border-border/80 p-4 pl-5 transition-colors hover:border-primary/45"
              >
                <div
                  className={cn(
                    'absolute inset-y-0 left-0 w-1',
                    plan.active ? 'bg-emerald-400 shadow-[0_0_10px_hsl(162_84%_43%/0.6)]' : 'bg-zinc-600',
                  )}
                  aria-hidden="true"
                />
                <div className="space-y-3">
                  <div className="flex items-start justify-between gap-2">
                    <div>
                      <div className="flex items-center gap-2">
                        <Sparkles className="h-4 w-4 text-primary" />
                        <span className="font-bold text-base">{plan.name}</span>
                      </div>
                      {plan.description && (
                        <p className="mt-0.5 text-xs text-muted-foreground">{plan.description}</p>
                      )}
                    </div>
                    <Badge variant={plan.active ? 'success' : 'secondary'}>
                      {plan.active ? (isRu ? 'Активен' : 'Active') : isRu ? 'Отключён' : 'Inactive'}
                    </Badge>
                  </div>

                  <div className="font-mono text-2xl font-bold tabular-nums">
                    {plan.price} {plan.currency}{' '}
                    <span className="text-xs font-normal text-muted-foreground">
                      / {dur} {isRu ? 'дн.' : 'd'}
                    </span>
                  </div>

                  <div className="rack-inset grid grid-cols-2 gap-2 rounded-lg p-3 text-xs">
                    <div>
                      <div className="text-[10px] text-muted-foreground">{isRu ? 'Трафик' : 'Traffic'}</div>
                      <div className="font-mono font-semibold">
                        {bytes > 0 ? formatBytes(bytes, { locale: language }) : isRu ? 'Безлимит' : 'Unlimited'}
                      </div>
                    </div>
                    <div>
                      <div className="text-[10px] text-muted-foreground">HWID Limit</div>
                      <div className="font-mono font-semibold">
                        {plan.hwidLimit !== undefined && plan.hwidLimit !== null ? plan.hwidLimit : '∞'}
                      </div>
                    </div>
                    <div>
                      <div className="text-[10px] text-muted-foreground">
                        {isRu ? 'Макс. клиентов' : 'Max Clients'}
                      </div>
                      <div className="font-mono font-semibold">
                        {plan.maxClients ?? plan.deviceLimit ?? 1}
                      </div>
                    </div>
                    <div>
                      <div className="text-[10px] text-muted-foreground">{isRu ? 'Сброс' : 'Reset'}</div>
                      <div className="font-mono font-semibold">{plan.defaultResetStrategy || 'NO_RESET'}</div>
                    </div>
                  </div>

                  <div className="flex flex-wrap gap-1">
                    {plan.protocols.map((p) => (
                      <Badge key={p} className={cn('border px-1.5 py-0 text-[10px]', PROTOCOL_COLORS[p])}>
                        {p}
                      </Badge>
                    ))}
                  </div>
                </div>

                <div className="mt-4 flex justify-end gap-1 border-t border-border/60 pt-2.5">
                  <Button size="sm" variant="outline" className="h-7 text-xs" onClick={() => openEdit(plan)}>
                    <Edit className="h-3.5 w-3.5" /> {isRu ? 'Изменить' : 'Edit'}
                  </Button>
                  <Button size="sm" variant="ghost" className="h-7 px-2" onClick={() => deleteMut.mutate(plan.id)}>
                    <Trash2 className="h-3.5 w-3.5 text-red-400" />
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      <Dialog open={modalOpen} onOpenChange={setModalOpen}>
        <DialogContent size="xl">
          <DialogHeader>
            <DialogTitle>
              {editing
                ? isRu
                  ? `Редактировать тариф: ${editing.name}`
                  : `Edit Plan: ${editing.name}`
                : isRu
                  ? 'Создать тарифный план'
                  : 'Create Plan'}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Название' : 'Name'}</Label>
                <Input value={name} onChange={(e) => setName(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Тип' : 'Type'}</Label>
                <Select value={type} onValueChange={(v) => setType(v as PlanType)}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="USER">USER</SelectItem>
                    <SelectItem value="RESELLER">RESELLER</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Цена' : 'Price'}</Label>
                <Input type="number" min={0} value={price} onChange={(e) => setPrice(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Валюта' : 'Currency'}</Label>
                <Input value={currency} onChange={(e) => setCurrency(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Длительность (дней)' : 'Duration (days)'}</Label>
                <Input
                  type="number"
                  min={1}
                  value={duration}
                  onChange={(e) => setDuration(e.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Лимит трафика (ГБ, 0 = безлимит)' : 'Traffic Limit (GB, 0 = unlimited)'}</Label>
                <Input
                  type="number"
                  min={0}
                  value={trafficGb}
                  onChange={(e) => setTrafficGb(e.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Лимит HWID-устройств (пусто = безлимит)' : 'HWID Device Limit (blank = unlimited)'}</Label>
                <Input
                  type="number"
                  min={0}
                  value={hwidLimit}
                  onChange={(e) => setHwidLimit(e.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Стратегия сброса трафика' : 'Default Reset Strategy'}</Label>
                <Select
                  value={defaultResetStrategy}
                  onValueChange={(v) => setDefaultResetStrategy(v as TrafficResetStrategy)}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {RESET_STRATEGIES.map((s) => (
                      <SelectItem key={s} value={s}>
                        {s}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Макс. клиентов' : 'Max Clients'}</Label>
                <Input
                  type="number"
                  min={1}
                  value={maxClients}
                  onChange={(e) => setMaxClients(e.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Макс. скорость (Мбит/с, 0 = безлимит)' : 'Max Speed (Mbps, 0 = unlimited)'}</Label>
                <Input
                  type="number"
                  min={0}
                  value={maxSpeed}
                  onChange={(e) => setMaxSpeed(e.target.value)}
                />
              </div>
            </div>

            <div className="space-y-1.5">
              <Label>{isRu ? 'Описание' : 'Description'}</Label>
              <Input value={description} onChange={(e) => setDescription(e.target.value)} />
            </div>

            <div className="space-y-2">
              <Label>{isRu ? 'Разрешённые протоколы' : 'Allowed Protocols'}</Label>
              <div className="flex flex-wrap gap-2">
                {PROTOCOLS.map((p) => (
                  <label key={p} className="flex cursor-pointer items-center gap-1.5 rounded border px-2 py-1 text-xs">
                    <Checkbox
                      checked={protocols.includes(p)}
                      onCheckedChange={() =>
                        setProtocols((prev) =>
                          prev.includes(p) ? prev.filter((x) => x !== p) : [...prev, p],
                        )
                      }
                    />
                    <span>{p}</span>
                  </label>
                ))}
              </div>
            </div>

            <div className="flex items-center justify-between rounded-lg border px-3 py-2">
              <Label>{isRu ? 'Активен' : 'Active'}</Label>
              <Switch checked={active} onCheckedChange={setActive} />
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
              {isRu ? 'Сохранить тариф' : 'Save Plan'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
