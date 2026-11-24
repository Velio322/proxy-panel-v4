import { useEffect, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Check, Copy, Sparkles } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Checkbox } from '@/components/ui/checkbox';
import { NodePicker } from '@/components/clients/NodePicker';
import { clientsApi, getApiErrorMessage, nodesApi } from '@/lib/api';
import { PROTOCOLS } from '@/lib/core-utils';
import { buildSubUrl, generateRandomPassword } from '@/pages/clients/utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { Client, Plan, Protocol, TrafficResetStrategy } from '@/types';

interface CreateClientModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  plans: Plan[];
  onCreated: () => void;
}

const RESET_STRATEGIES: TrafficResetStrategy[] = [
  'NO_RESET',
  'DAY',
  'WEEK',
  'MONTH',
  'MONTH_ROLLING',
  'ON_HOLD',
];

export function CreateClientModal({ open, onOpenChange, plans, onCreated }: CreateClientModalProps) {
  const isRu = useUiStore((s) => s.language) === 'ru';
  const [username, setUsername] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [planId, setPlanId] = useState<string>('none');
  const [protocols, setProtocols] = useState<Protocol[]>(['VLESS']);
  const [allNodes, setAllNodes] = useState(true);
  const [selectedNodeIds, setSelectedNodeIds] = useState<string[]>([]);
  const [trafficLimitGb, setTrafficLimitGb] = useState('0');
  const [expireDays, setExpireDays] = useState('30');
  const [resetStrategy, setResetStrategy] = useState<TrafficResetStrategy>('NO_RESET');
  const [trafficResetDay, setTrafficResetDay] = useState('1');
  const [deviceLimit, setDeviceLimit] = useState('');
  const [note, setNote] = useState('');
  const [saving, setSaving] = useState(false);

  const [createdCreds, setCreatedCreds] = useState<{
    client: Client;
    password?: string;
    subUrl: string;
  } | null>(null);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
    enabled: open,
  });

  useEffect(() => {
    if (!open) {
      setUsername('');
      setEmail('');
      setPassword('');
      setPlanId('none');
      setProtocols(['VLESS']);
      setAllNodes(true);
      setSelectedNodeIds([]);
      setTrafficLimitGb('0');
      setExpireDays('30');
      setResetStrategy('NO_RESET');
      setTrafficResetDay('1');
      setDeviceLimit('');
      setNote('');
      setCreatedCreds(null);
    }
  }, [open]);

  const handlePlanSelect = (selectedId: string) => {
    setPlanId(selectedId);
    if (selectedId === 'none') return;
    const plan = plans.find((p) => p.id === selectedId);
    if (!plan) return;
    const bytes = plan.trafficLimit ?? plan.trafficLimitBytes ?? 0;
    setTrafficLimitGb(bytes > 0 ? String(Math.round(bytes / (1024 * 1024 * 1024))) : '0');
    const days = plan.duration ?? plan.periodDays ?? 30;
    setExpireDays(days > 0 ? String(days) : '');
    if (plan.protocols?.length) setProtocols(plan.protocols);
    if (plan.hwidLimit !== undefined && plan.hwidLimit !== null) {
      setDeviceLimit(String(plan.hwidLimit));
    }
    if (plan.defaultResetStrategy) {
      setResetStrategy(plan.defaultResetStrategy);
    }
  };

  const toggleProtocol = (proto: Protocol) => {
    setProtocols((prev) =>
      prev.includes(proto) ? prev.filter((p) => p !== proto) : [...prev, proto],
    );
  };

  const handleCopy = async (key: string, val: string) => {
    await navigator.clipboard.writeText(val);
    setCopiedKey(key);
    window.setTimeout(() => setCopiedKey(null), 1500);
  };

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!username.trim()) return;
    if (protocols.length === 0) {
      toast({ title: isRu ? 'Выберите хотя бы один протокол' : 'Select at least one protocol', variant: 'destructive' });
      return;
    }
    if (!allNodes && selectedNodeIds.length === 0) {
      toast({
        title: isRu ? 'Выберите ноды или отметьте «Все ноды»' : 'Select at least one node or choose All Nodes',
        variant: 'destructive',
      });
      return;
    }

    setSaving(true);
    try {
      const gb = Number(trafficLimitGb) || 0;
      const trafficLimit = gb > 0 ? Math.round(gb * 1024 * 1024 * 1024) : 0;
      const days = Number(expireDays);
      const expireAt =
        expireDays.trim() && Number.isFinite(days) && days > 0
          ? new Date(Date.now() + days * 86400_000).toISOString()
          : null;

      const created = await clientsApi.create({
        username: username.trim(),
        email: email.trim() || undefined,
        password: password.trim() || undefined,
        planId: planId !== 'none' ? planId : undefined,
        protocols,
        allNodes,
        nodeIds: allNodes ? [] : selectedNodeIds,
        trafficLimit,
        expireAt,
        resetStrategy,
        onHold: resetStrategy === 'ON_HOLD',
        trafficResetDay:
          resetStrategy === 'MONTH' ? Math.max(1, Math.min(28, Number(trafficResetDay) || 1)) : undefined,
        deviceLimit: deviceLimit.trim() !== '' ? Number(deviceLimit) : null,
        note: note.trim() || undefined,
      });

      setCreatedCreds({
        client: created,
        password: created.password || password.trim() || undefined,
        subUrl: buildSubUrl(created.subToken),
      });
      onCreated();
      toast({ title: isRu ? 'Клиент создан' : 'Client created', variant: 'success' });
    } catch (err) {
      toast({
        title: isRu ? 'Ошибка создания' : 'Create failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent size="2xl">
        <DialogHeader>
          <DialogTitle>
            {createdCreds
              ? isRu
                ? 'Клиент создан — данные подключения'
                : 'Client Credentials'
              : isRu
                ? 'Новый клиент'
                : 'Create Client'}
          </DialogTitle>
          <DialogDescription>
            {createdCreds
              ? isRu
                ? 'Скопируйте данные доступа и ссылку подписки ниже.'
                : 'Save the generated credentials and subscription link below.'
              : isRu
                ? 'Настройте имя, протоколы, привязку к нодам и лимиты трафика.'
                : 'Provision a new proxy client with node binding, traffic limit, HWID cap, and reset strategy.'}
          </DialogDescription>
        </DialogHeader>

        {createdCreds ? (
          <div className="space-y-3">
            <div className="rounded-lg border bg-muted/20 p-3.5 space-y-2 text-xs">
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground">{isRu ? 'Имя пользователя' : 'Username'}</span>
                <span className="font-semibold">{createdCreds.client.username}</span>
              </div>
              <div className="flex items-center justify-between gap-2">
                <span className="text-muted-foreground">UUID</span>
                <div className="flex items-center gap-1 font-mono">
                  <span>{createdCreds.client.uuid}</span>
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    className="h-6 w-6"
                    onClick={() => handleCopy('uuid', createdCreds.client.uuid)}
                  >
                    {copiedKey === 'uuid' ? <Check className="h-3 w-3 text-emerald-400" /> : <Copy className="h-3 w-3" />}
                  </Button>
                </div>
              </div>
              {createdCreds.password && (
                <div className="flex items-center justify-between gap-2">
                  <span className="text-muted-foreground">{isRu ? 'Пароль' : 'Password'}</span>
                  <div className="flex items-center gap-1 font-mono">
                    <span>{createdCreds.password}</span>
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      className="h-6 w-6"
                      onClick={() => handleCopy('pass', createdCreds.password!)}
                    >
                      {copiedKey === 'pass' ? <Check className="h-3 w-3 text-emerald-400" /> : <Copy className="h-3 w-3" />}
                    </Button>
                  </div>
                </div>
              )}
              <div className="space-y-1 pt-1">
                <span className="text-muted-foreground">{isRu ? 'Ссылка подписки' : 'Subscription URL'}</span>
                <div className="flex gap-1.5">
                  <Input readOnly value={createdCreds.subUrl} className="font-mono text-xs" />
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => handleCopy('sub', createdCreds.subUrl)}
                  >
                    {copiedKey === 'sub' ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
                  </Button>
                </div>
              </div>
            </div>
            <DialogFooter>
              <Button type="button" onClick={() => onOpenChange(false)}>
                {isRu ? 'Готово' : 'Done'}
              </Button>
            </DialogFooter>
          </div>
        ) : (
          <form onSubmit={handleCreate} className="space-y-4">
            <div className="grid gap-2 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Имя клиента' : 'Username'}</Label>
                <Input
                  required
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  placeholder="alice"
                />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Email (необязательно)' : 'Email (optional)'}</Label>
                <Input
                  type="email"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="alice@example.com"
                />
              </div>
              <div className="space-y-1.5">
                <div className="flex items-center justify-between">
                  <Label>{isRu ? 'Пароль (необязательно)' : 'Password (optional)'}</Label>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="h-5 px-1.5 text-xs"
                    onClick={() => setPassword(generateRandomPassword(16))}
                  >
                    <Sparkles className="h-3 w-3" /> {isRu ? 'Сгенерировать' : 'Generate'}
                  </Button>
                </div>
                <Input
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder={isRu ? 'Автогенерация, если пусто' : 'Auto-generated if blank'}
                />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Шаблон тарифа' : 'Plan Template'}</Label>
                <Select value={planId} onValueChange={handlePlanSelect}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="none">{isRu ? 'Без тарифа (вручную)' : 'Custom (No Plan)'}</SelectItem>
                    {plans.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Лимит трафика (ГБ, 0 = безлимит)' : 'Traffic Limit (GB, 0 = unlimited)'}</Label>
                <Input
                  type="number"
                  min={0}
                  value={trafficLimitGb}
                  onChange={(e) => setTrafficLimitGb(e.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Срок действия (дней, пусто = бессрочно)' : 'Expire in Days (blank = never)'}</Label>
                <Input
                  type="number"
                  min={0}
                  value={expireDays}
                  onChange={(e) => setExpireDays(e.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Сброс трафика' : 'Traffic Reset Strategy'}</Label>
                <Select
                  value={resetStrategy}
                  onValueChange={(v) => setResetStrategy(v as TrafficResetStrategy)}
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
              {resetStrategy === 'MONTH' ? (
                <div className="space-y-1.5">
                  <Label>{isRu ? 'День сброса (1–28)' : 'Reset Day (1–28)'}</Label>
                  <Input
                    type="number"
                    min={1}
                    max={28}
                    value={trafficResetDay}
                    onChange={(e) => setTrafficResetDay(e.target.value)}
                  />
                </div>
              ) : (
                <div className="space-y-1.5">
                  <Label>{isRu ? 'Лимит устройств HWID' : 'HWID Device Limit (blank = plan, 0 = unlimited)'}</Label>
                  <Input
                    type="number"
                    min={0}
                    placeholder={isRu ? 'Из тарифа / без лимита' : 'Inherit from plan'}
                    value={deviceLimit}
                    onChange={(e) => setDeviceLimit(e.target.value)}
                  />
                </div>
              )}
            </div>

            <div className="space-y-2">
              <Label>{isRu ? 'Разрешённые протоколы' : 'Enabled Protocols'}</Label>
              <div className="flex flex-wrap gap-2">
                {PROTOCOLS.map((proto) => {
                  const checked = protocols.includes(proto);
                  return (
                    <label
                      key={proto}
                      className="flex cursor-pointer items-center gap-1.5 rounded-md border px-2.5 py-1 text-xs hover:bg-accent/40"
                    >
                      <Checkbox checked={checked} onCheckedChange={() => toggleProtocol(proto)} />
                      <span>{proto}</span>
                    </label>
                  );
                })}
              </div>
            </div>

            <div className="space-y-2">
              <Label>{isRu ? 'Привязка к нодам' : 'Node Binding'}</Label>
              <NodePicker
                nodes={nodes}
                allNodes={allNodes}
                selectedNodeIds={selectedNodeIds}
                onAllNodesChange={setAllNodes}
                onSelectedNodeIdsChange={setSelectedNodeIds}
              />
            </div>

            <div className="space-y-1.5">
              <Label>{isRu ? 'Заметка' : 'Note'}</Label>
              <Input
                value={note}
                onChange={(e) => setNote(e.target.value)}
                placeholder={isRu ? 'Внутренний комментарий' : 'Internal note'}
              />
            </div>

            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
                {isRu ? 'Отмена' : 'Cancel'}
              </Button>
              <Button type="submit" loading={saving}>
                {isRu ? 'Создать клиента' : 'Create Client'}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
