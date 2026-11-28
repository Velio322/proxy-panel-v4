import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Download, RefreshCw, Search, Smartphone, Trash2 } from 'lucide-react';
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
import { Switch } from '@/components/ui/switch';
import { Checkbox } from '@/components/ui/checkbox';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { NodePicker } from '@/components/clients/NodePicker';
import { clientsApi, getApiErrorMessage, inboundsApi, nodesApi } from '@/lib/api';
import { PROTOCOLS, formatDateTime } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { Client, Inbound, Plan, Protocol, TrafficResetStrategy } from '@/types';

export type EditClientTab = 'profile' | 'nodes' | 'inbounds' | 'devices' | 'exports';

interface EditClientModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  client: Client | null;
  plans: Plan[];
  initialTab?: EditClientTab;
  onSaved: () => void;
}

const RESET_STRATEGIES: TrafficResetStrategy[] = [
  'NO_RESET',
  'DAY',
  'WEEK',
  'MONTH',
  'MONTH_ROLLING',
  'ON_HOLD',
];

export function EditClientModal({
  open,
  onOpenChange,
  client,
  plans,
  initialTab = 'profile',
  onSaved,
}: EditClientModalProps) {
  const qc = useQueryClient();
  const timezone = useUiStore((s) => s.timezone);
  const isRu = useUiStore((s) => s.language) === 'ru';

  const [activeTab, setActiveTab] = useState<EditClientTab>(initialTab);
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [note, setNote] = useState('');
  const [protocols, setProtocols] = useState<Protocol[]>(['VLESS']);
  const [trafficLimitGb, setTrafficLimitGb] = useState('0');
  const [expireDate, setExpireDate] = useState('');
  const [banned, setBanned] = useState(false);
  const [planId, setPlanId] = useState('none');
  const [deviceLimit, setDeviceLimit] = useState('');
  const [resetStrategy, setResetStrategy] = useState<TrafficResetStrategy>('NO_RESET');
  const [trafficResetDay, setTrafficResetDay] = useState('1');
  const [restrictionMode, setRestrictionMode] = useState<'SOFT' | 'HARD'>('HARD');
  const [allNodes, setAllNodes] = useState(true);
  const [selectedNodeIds, setSelectedNodeIds] = useState<string[]>([]);
  const [nodesDirty, setNodesDirty] = useState(false);
  const [selectedInbounds, setSelectedInbounds] = useState<string[]>([]);
  const [inboundsDirty, setInboundsDirty] = useState(false);
  const [inboundSearch, setInboundSearch] = useState('');
  const [exportPreview, setExportPreview] = useState('');
  const [activeSubToken, setActiveSubToken] = useState('');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open || !client) return;
    setActiveTab(initialTab);
    setActiveSubToken(client.subToken);
    setEmail(client.email ?? '');
    setPassword('');
    setNote(client.note ?? '');
    setProtocols(client.protocols?.length ? client.protocols : ['VLESS']);
    setTrafficLimitGb(
      client.trafficLimit > 0
        ? String(Math.round(client.trafficLimit / (1024 * 1024 * 1024)))
        : '0',
    );
    setExpireDate(client.expireAt ? client.expireAt.slice(0, 10) : '');
    setBanned(client.banned);
    setPlanId(client.planId || 'none');
    setDeviceLimit(
      client.deviceLimit !== undefined && client.deviceLimit !== null
        ? String(client.deviceLimit)
        : '',
    );
    setResetStrategy(client.resetStrategy ?? (client.onHold ? 'ON_HOLD' : 'NO_RESET'));
    setTrafficResetDay(String(client.trafficResetDay ?? 1));
    setRestrictionMode(client.restrictionMode === 'SOFT' ? 'SOFT' : 'HARD');
    setAllNodes(client.allNodes ?? true);
    setSelectedNodeIds(client.nodeIds ?? []);
    setNodesDirty(false);
    setInboundsDirty(false);
    setInboundSearch('');
    setExportPreview('');
  }, [open, client, initialTab]);

  const { data: allNodesList = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
    enabled: open,
  });

  const { data: clientNodes } = useQuery({
    queryKey: ['client-nodes', client?.id],
    queryFn: () => clientsApi.getNodes(client!.id),
    enabled: open && Boolean(client?.id),
  });

  useEffect(() => {
    if (clientNodes && !nodesDirty) {
      setAllNodes(clientNodes.allNodes);
      setSelectedNodeIds(clientNodes.nodeIds ?? []);
    }
  }, [clientNodes, nodesDirty]);

  const { data: allInbounds = [] } = useQuery({
    queryKey: ['inbounds'],
    queryFn: () => inboundsApi.list(),
    enabled: open,
  });

  const { data: clientInbounds } = useQuery({
    queryKey: ['client-inbounds', client?.id],
    queryFn: () => clientsApi.getInbounds(client!.id),
    enabled: open && Boolean(client?.id),
  });

  useEffect(() => {
    if (clientInbounds?.inboundIds && !inboundsDirty) {
      setSelectedInbounds(clientInbounds.inboundIds);
    }
  }, [clientInbounds, inboundsDirty]);

  const groupedInbounds = useMemo(() => {
    const allowedSet = new Set(selectedNodeIds);
    const q = inboundSearch.trim().toLowerCase();
    const visible = allInbounds.filter((ib) => {
      if (!allNodes && !allowedSet.has(ib.nodeId)) return false;
      if (!q) return true;
      const nodeName = ib.node?.name ?? ib.nodeId;
      return `${ib.tag} ${ib.protocol} ${nodeName} ${ib.port}`.toLowerCase().includes(q);
    });
    const map = new Map<string, { nodeName: string; items: Inbound[] }>();
    for (const ib of visible) {
      const nodeName = ib.node?.name ?? ib.nodeId;
      const group = map.get(ib.nodeId) ?? { nodeName, items: [] };
      group.items.push(ib);
      map.set(ib.nodeId, group);
    }
    return Array.from(map.entries())
      .map(([nodeId, val]) => ({
        nodeId,
        nodeName: val.nodeName,
        items: val.items.sort((a, b) => a.port - b.port),
      }))
      .sort((a, b) => a.nodeName.localeCompare(b.nodeName));
  }, [allInbounds, allNodes, selectedNodeIds, inboundSearch]);

  const { data: devices = [], isLoading: devicesLoading } = useQuery({
    queryKey: ['client-devices', client?.id],
    queryFn: () => clientsApi.listDevices(client!.id),
    enabled: open && Boolean(client?.id),
  });

  const deleteDeviceMut = useMutation({
    mutationFn: (deviceId: string) => clientsApi.deleteDevice(client!.id, deviceId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['client-devices', client?.id] });
      toast({ title: isRu ? 'Устройство удалено' : 'Device removed', variant: 'success' });
    },
    onError: (err) => toast({ title: isRu ? 'Ошибка' : 'Failed', description: getApiErrorMessage(err), variant: 'destructive' }),
  });

  const resetDevicesMut = useMutation({
    mutationFn: () => clientsApi.resetDevices(client!.id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['client-devices', client?.id] });
      toast({ title: isRu ? 'Все устройства сброшены' : 'All HWID devices cleared', variant: 'success' });
    },
    onError: (err) => toast({ title: isRu ? 'Ошибка' : 'Failed', description: getApiErrorMessage(err), variant: 'destructive' }),
  });

  const regenTokenMut = useMutation({
    mutationFn: () => clientsApi.regenerateSubToken(activeSubToken || client!.subToken),
    onSuccess: (res) => {
      if (res?.subToken) setActiveSubToken(res.subToken);
      onSaved();
      toast({ title: isRu ? 'Ссылка подписки перевыпущена' : 'Subscription token regenerated', variant: 'success' });
    },
    onError: (err) => toast({ title: isRu ? 'Ошибка' : 'Failed', description: getApiErrorMessage(err), variant: 'destructive' }),
  });

  if (!client) return null;

  const effectiveLimit =
    client.deviceLimit !== undefined && client.deviceLimit !== null
      ? client.deviceLimit === 0
        ? isRu
          ? 'Безлимит'
          : 'Unlimited'
        : String(client.deviceLimit)
      : client.plan?.hwidLimit
        ? `${client.plan.hwidLimit} (${isRu ? 'Тариф' : 'Plan'})`
        : isRu
          ? 'Безлимит'
          : 'Unlimited';

  const handleSave = async () => {
    setSaving(true);
    try {
      const gb = Number(trafficLimitGb) || 0;
      await clientsApi.update(client.id, {
        email: email.trim() || null,
        ...(password.trim() ? { password: password.trim() } : {}),
        note: note.trim() || null,
        protocols,
        trafficLimit: gb > 0 ? Math.round(gb * 1024 * 1024 * 1024) : 0,
        expireAt: expireDate ? new Date(`${expireDate}T23:59:59Z`).toISOString() : null,
        banned,
        planId: planId !== 'none' ? planId : null,
        deviceLimit: deviceLimit.trim() !== '' ? Number(deviceLimit) : null,
        resetStrategy,
        onHold: resetStrategy === 'ON_HOLD',
        trafficResetDay:
          resetStrategy === 'MONTH' ? Math.max(1, Math.min(28, Number(trafficResetDay) || 1)) : null,
        restrictionMode,
      });
      if (nodesDirty) {
        await clientsApi.updateNodes(client.id, {
          allNodes,
          nodeIds: allNodes ? [] : selectedNodeIds,
          syncInbounds: !inboundsDirty,
        });
      }
      if (inboundsDirty) {
        const allowedNodeSet = new Set(selectedNodeIds);
        const filteredInbounds = allNodes
          ? selectedInbounds
          : selectedInbounds.filter((iid) => {
              const ib = allInbounds.find((x) => x.id === iid);
              return !ib || allowedNodeSet.has(ib.nodeId);
            });
        await clientsApi.updateInbounds(client.id, filteredInbounds);
      }
      qc.invalidateQueries({ queryKey: ['client-nodes', client.id] });
      qc.invalidateQueries({ queryKey: ['client-inbounds', client.id] });
      onSaved();
      onOpenChange(false);
      toast({ title: isRu ? 'Клиент сохранён' : 'Client updated', variant: 'success' });
    } catch (err) {
      toast({
        title: isRu ? 'Ошибка сохранения' : 'Update failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      });
    } finally {
      setSaving(false);
    }
  };

  const handleExport = async (fmt: 'uci' | 'passwall' | 'openclash' | 'singbox' | 'ssr') => {
    try {
      const text = await clientsApi.exportConfig(client.id, fmt);
      setExportPreview(text);
    } catch (err) {
      toast({
        title: isRu ? 'Ошибка экспорта' : 'Export failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      });
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent size="2xl">
        <DialogHeader>
          <DialogTitle>
            {isRu ? `Клиент: ${client.username}` : `Edit Client: ${client.username}`}
          </DialogTitle>
          <DialogDescription>
            {isRu
              ? 'Профиль, лимиты, привязка к нодам и протоколам, устройства и сброс токена.'
              : 'Update profile limits, node/inbound bindings, HWID devices, and OpenWrt exports.'}
          </DialogDescription>
        </DialogHeader>

        <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as EditClientTab)}>
          <TabsList className="flex-wrap">
            <TabsTrigger value="profile">{isRu ? 'Профиль и лимиты' : 'Profile & Limits'}</TabsTrigger>
            <TabsTrigger value="nodes">
              {isRu ? 'Ноды' : 'Nodes'} ({allNodes ? (isRu ? 'Все' : 'All') : selectedNodeIds.length})
            </TabsTrigger>
            <TabsTrigger value="inbounds">
              {isRu ? 'Порты / Инбаунды' : 'Inbounds'} ({selectedInbounds.length})
            </TabsTrigger>
            <TabsTrigger value="devices">
              {isRu ? 'Устройства' : 'Devices'} ({devices.length})
            </TabsTrigger>
            <TabsTrigger value="exports">{isRu ? 'OpenWrt и токен' : 'OpenWrt & Token'}</TabsTrigger>
          </TabsList>

          <TabsContent value="profile" className="space-y-4">
            <div className="grid gap-2 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>Email</Label>
                <Input value={email} onChange={(e) => setEmail(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Новый пароль (необязательно)' : 'New Password (optional)'}</Label>
                <Input
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder={isRu ? 'Оставьте пустым, чтобы не менять' : 'Leave blank to keep existing'}
                />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Тариф' : 'Plan'}</Label>
                <Select value={planId} onValueChange={setPlanId}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="none">{isRu ? 'Без тарифа' : 'No Plan'}</SelectItem>
                    {plans.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Лимит устройств HWID' : 'HWID Device Limit'}</Label>
                <Input
                  type="number"
                  min={0}
                  placeholder={isRu ? 'Из тарифа / без лимита' : 'Inherit from plan'}
                  value={deviceLimit}
                  onChange={(e) => setDeviceLimit(e.target.value)}
                />
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
                <Label>{isRu ? 'Дата окончания (пусто = бессрочно)' : 'Expiration Date (blank = never)'}</Label>
                <div className="flex gap-1.5">
                  <Input
                    type="date"
                    value={expireDate}
                    onChange={(e) => setExpireDate(e.target.value)}
                  />
                  {expireDate && (
                    <Button type="button" variant="outline" size="sm" onClick={() => setExpireDate('')}>
                      {isRu ? 'Сброс' : 'Clear'}
                    </Button>
                  )}
                </div>
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
              <div className="space-y-1.5">
                <Label>{isRu ? 'Режим ограничения' : 'Restriction Mode'}</Label>
                <Select
                  value={restrictionMode}
                  onValueChange={(v) => setRestrictionMode(v as 'SOFT' | 'HARD')}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="HARD">{isRu ? 'HARD (Отключать доступ)' : 'HARD (Disconnect)'}</SelectItem>
                    <SelectItem value="SOFT">{isRu ? 'SOFT (Мягкий режим)' : 'SOFT (Throttle/Notice)'}</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              {resetStrategy === 'MONTH' && (
                <div className="space-y-1.5">
                  <Label>{isRu ? 'День сброса в месяце (1–28)' : 'Monthly Reset Day (1–28)'}</Label>
                  <Input
                    type="number"
                    min={1}
                    max={28}
                    value={trafficResetDay}
                    onChange={(e) => setTrafficResetDay(e.target.value)}
                  />
                </div>
              )}
            </div>

            <div className="space-y-2">
              <Label>{isRu ? 'Протоколы' : 'Protocols'}</Label>
              <div className="flex flex-wrap gap-2">
                {PROTOCOLS.map((proto) => (
                  <label
                    key={proto}
                    className="flex cursor-pointer items-center gap-1.5 rounded-md border px-2.5 py-1 text-xs"
                  >
                    <Checkbox
                      checked={protocols.includes(proto)}
                      onCheckedChange={() =>
                        setProtocols((prev) =>
                          prev.includes(proto) ? prev.filter((p) => p !== proto) : [...prev, proto],
                        )
                      }
                    />
                    <span>{proto}</span>
                  </label>
                ))}
              </div>
            </div>

            <div className="grid gap-2 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Заметка' : 'Note'}</Label>
                <Input value={note} onChange={(e) => setNote(e.target.value)} />
              </div>
              <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                <Label>{isRu ? 'Заблокирован' : 'Banned'}</Label>
                <Switch checked={banned} onCheckedChange={setBanned} />
              </div>
            </div>
          </TabsContent>

          <TabsContent value="nodes" className="space-y-3">
            <div className="text-xs text-muted-foreground">
              {isRu
                ? 'Укажите, на каких нодах может работать этот клиент. Нужные протоколы (инбаунды) подхватятся автоматически после сохранения.'
                : 'Choose whether this client uses all active nodes or only pinned nodes. Saving automatically syncs matching inbounds unless you customize individual inbounds.'}
            </div>
            <NodePicker
              nodes={allNodesList}
              allNodes={allNodes}
              selectedNodeIds={selectedNodeIds}
              onAllNodesChange={(val) => {
                setAllNodes(val);
                setNodesDirty(true);
              }}
              onSelectedNodeIdsChange={(ids) => {
                setSelectedNodeIds(ids);
                setNodesDirty(true);
              }}
            />
          </TabsContent>

          <TabsContent value="inbounds" className="space-y-3">
            <div className="relative">
              <Search className="absolute left-2.5 top-2.5 h-3.5 w-3.5 text-muted-foreground" />
              <Input
                value={inboundSearch}
                onChange={(e) => setInboundSearch(e.target.value)}
                placeholder={isRu ? 'Фильтр по ноде, тегу, протоколу, порту...' : 'Filter inbounds by node, tag, protocol, port...'}
                className="h-8 pl-8 text-xs"
              />
            </div>
            <div className="max-h-64 overflow-y-auto space-y-3 pr-1">
              {groupedInbounds.length === 0 ? (
                <div className="py-6 text-center text-xs text-muted-foreground">
                  {!allNodes && selectedNodeIds.length === 0
                    ? isRu
                      ? 'Сначала выберите хотя бы одну ноду во вкладке «Ноды».'
                      : 'Select at least one node in the Nodes tab first.'
                    : isRu
                      ? 'Подходящих инбаундов не найдено.'
                      : 'No matching inbounds available.'}
                </div>
              ) : (
                groupedInbounds.map((group) => (
                  <div key={group.nodeId} className="space-y-1.5">
                    <div className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground px-1">
                      {group.nodeName}
                    </div>
                    {group.items.map((ib) => (
                      <label
                        key={ib.id}
                        className="flex cursor-pointer items-center justify-between rounded-lg border px-3 py-2 text-sm hover:bg-accent/40"
                      >
                        <div className="flex items-center gap-2.5">
                          <Checkbox
                            checked={selectedInbounds.includes(ib.id)}
                            onCheckedChange={() => {
                              setInboundsDirty(true);
                              setSelectedInbounds((prev) =>
                                prev.includes(ib.id)
                                  ? prev.filter((id) => id !== ib.id)
                                  : [...prev, ib.id],
                              );
                            }}
                          />
                          <span className="font-medium">{ib.tag}</span>
                          <Badge variant="outline">{ib.protocol}</Badge>
                        </div>
                        <span className="text-xs font-mono text-muted-foreground">:{ib.port}</span>
                      </label>
                    ))}
                  </div>
                ))
              )}
            </div>
          </TabsContent>

          <TabsContent value="devices" className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="text-xs text-muted-foreground">
                {isRu ? 'Действующий лимит HWID:' : 'Effective HWID limit:'}{' '}
                <span className="font-semibold text-foreground">{effectiveLimit}</span>
              </div>
              {devices.length > 0 && (
                <Button
                  type="button"
                  size="sm"
                  variant="destructive"
                  onClick={() => resetDevicesMut.mutate()}
                  loading={resetDevicesMut.isPending}
                >
                  {isRu ? 'Сбросить все устройства' : 'Reset All Devices'}
                </Button>
              )}
            </div>
            {devicesLoading ? (
              <div className="py-6 text-center text-xs text-muted-foreground">
                {isRu ? 'Загрузка устройств...' : 'Loading devices...'}
              </div>
            ) : devices.length === 0 ? (
              <div className="rounded-lg border border-dashed py-6 text-center text-xs text-muted-foreground">
                {isRu ? 'Привязанных HWID-устройств пока нет.' : 'No registered HWID devices yet.'}
              </div>
            ) : (
              <div className="space-y-2 max-h-64 overflow-y-auto">
                {devices.map((d) => (
                  <div key={d.id} className="flex items-center justify-between gap-2 rounded-lg border p-3">
                    <div className="min-w-0 space-y-0.5">
                      <div className="flex items-center gap-2 text-xs font-semibold">
                        <Smartphone className="h-3.5 w-3.5 text-primary" />
                        <span className="font-mono">{d.hwid}</span>
                        {d.os && <Badge variant="outline">{d.os} {d.osVersion || ''}</Badge>}
                      </div>
                      <div className="text-[11px] text-muted-foreground">
                        {d.model || d.userAgent || (isRu ? 'Неизвестный клиент' : 'Unknown client')} · {d.ip || '—'} ·{' '}
                        {formatDateTime(d.lastSeenAt, timezone)}
                      </div>
                    </div>
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      aria-label={`Delete device ${d.hwid}`}
                      onClick={() => deleteDeviceMut.mutate(d.id)}
                    >
                      <Trash2 className="h-4 w-4 text-red-400" />
                    </Button>
                  </div>
                ))}
              </div>
            )}
          </TabsContent>

          <TabsContent value="exports" className="space-y-4">
            <div className="flex flex-wrap gap-2">
              {(['uci', 'passwall', 'openclash', 'singbox', 'ssr'] as const).map((fmt) => (
                <Button key={fmt} type="button" size="sm" variant="outline" onClick={() => handleExport(fmt)}>
                  <Download className="h-3.5 w-3.5" /> {fmt.toUpperCase()}
                </Button>
              ))}
            </div>
            {exportPreview && (
              <pre className="max-h-48 overflow-auto rounded-lg border bg-background p-3 font-mono text-xs">
                {exportPreview}
              </pre>
            )}
            <div className="flex items-center justify-between rounded-lg border border-amber-500/30 bg-amber-500/5 p-3">
              <div>
                <div className="text-xs font-semibold">
                  {isRu ? 'Перевыпустить токен подписки' : 'Regenerate Subscription Token'}
                </div>
                <div className="text-[11px] text-muted-foreground">
                  {isRu
                    ? 'Старая ссылка подписки сразу перестанет работать.'
                    : 'Invalidates the existing subscription URL immediately.'}
                </div>
              </div>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => regenTokenMut.mutate()}
                loading={regenTokenMut.isPending}
              >
                <RefreshCw className="h-3.5 w-3.5" /> {isRu ? 'Перевыпустить' : 'Regenerate'}
              </Button>
            </div>
          </TabsContent>
        </Tabs>

        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            {isRu ? 'Отмена' : 'Cancel'}
          </Button>
          <Button type="button" onClick={handleSave} loading={saving}>
            {isRu ? 'Сохранить изменения' : 'Save Changes'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
