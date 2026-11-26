import { useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  ArrowDown,
  ArrowUp,
  Edit,
  Plus,
  Power,
  ShieldAlert,
  Sparkles,
  Trash2,
  Waypoints,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
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
import { PageHero, RackEmptyState } from '@/components/ui/PagePrimitives';
import { getApiErrorMessage, nodesApi, routingApi } from '@/lib/api';
import { cn } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { RoutingRule } from '@/types';

export function RoutingPage() {
  const qc = useQueryClient();
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<RoutingRule | null>(null);

  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [outboundTag, setOutboundTag] = useState('block');
  const [nodeScope, setNodeScope] = useState('all');
  const [domains, setDomains] = useState('');
  const [ips, setIps] = useState('');
  const [port, setPort] = useState('');
  const [protocols, setProtocols] = useState('');
  const [ruleSet, setRuleSet] = useState('');
  const [enabled, setEnabled] = useState(true);

  const { data: rules = [], isLoading } = useQuery({
    queryKey: ['routing'],
    queryFn: () => routingApi.list(),
  });

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
  });

  const { data: presets = [] } = useQuery({
    queryKey: ['routing-presets'],
    queryFn: () => routingApi.presets(),
  });

  const stats = useMemo(() => {
    let active = 0;
    let blockCount = 0;
    let directCount = 0;
    let proxyCount = 0;
    for (const r of rules) {
      if (r.enabled) active += 1;
      if (r.outboundTag === 'block') blockCount += 1;
      else if (r.outboundTag === 'direct') directCount += 1;
      else proxyCount += 1;
    }
    return {
      total: rules.length,
      active,
      disabled: rules.length - active,
      blockCount,
      directCount,
      proxyCount,
    };
  }, [rules]);

  const openCreate = () => {
    setEditing(null);
    setName('');
    setDescription('');
    setOutboundTag('block');
    setNodeScope('all');
    setDomains('');
    setIps('');
    setPort('');
    setProtocols('');
    setRuleSet('');
    setEnabled(true);
    setModalOpen(true);
  };

  const openEdit = (r: RoutingRule) => {
    setEditing(r);
    setName(r.name);
    setDescription(r.description ?? '');
    setOutboundTag(r.outboundTag || 'block');
    setNodeScope(r.nodeScope || 'all');
    setDomains((r.domain ?? []).join(', '));
    setIps((r.ip ?? []).join(', '));
    setPort(r.port ?? '');
    setProtocols((r.protocol ?? []).join(', '));
    setRuleSet((r.ruleSet ?? []).join(', '));
    setEnabled(r.enabled);
    setModalOpen(true);
  };

  const splitList = (raw: string) =>
    raw
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);

  const saveMut = useMutation({
    mutationFn: async () => {
      const payload: Partial<RoutingRule> = {
        name: name.trim(),
        description: description.trim() || null,
        enabled,
        priority: editing?.priority ?? rules.length + 1,
        type: 'field',
        outboundTag,
        nodeScope: nodeScope === 'all' ? null : nodeScope,
        domain: splitList(domains),
        ip: splitList(ips),
        port: port.trim() || null,
        protocol: splitList(protocols),
        ruleSet: splitList(ruleSet),
      };
      if (editing) return routingApi.update(editing.id, payload);
      return routingApi.create(payload);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['routing'] });
      setModalOpen(false);
      toast({
        title: editing
          ? isRu
            ? 'Правило обновлено'
            : 'Rule updated'
          : isRu
            ? 'Правило создано'
            : 'Rule created',
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

  const reorderMut = useMutation({
    mutationFn: (nextRules: RoutingRule[]) =>
      routingApi.reorder(nextRules.map((r, idx) => ({ id: r.id, priority: idx + 1 }))),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['routing'] }),
  });

  const toggleMut = useMutation({
    mutationFn: (id: string) => routingApi.toggle(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['routing'] }),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => routingApi.delete(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['routing'] });
      toast({ title: isRu ? 'Правило удалено' : 'Rule deleted', variant: 'success' });
    },
  });

  const applyPresetMut = useMutation({
    mutationFn: (rule: Partial<RoutingRule>) => routingApi.create(rule),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['routing'] });
      toast({ title: isRu ? 'Пресет добавлен' : 'Preset rule added', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Не удалось применить пресет' : 'Failed to apply preset',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const moveRule = (index: number, delta: -1 | 1) => {
    const target = index + delta;
    if (target < 0 || target >= rules.length) return;
    const copy = [...rules];
    const [moved] = copy.splice(index, 1);
    copy.splice(target, 0, moved);
    reorderMut.mutate(copy);
  };

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Правила маршрутизации' : 'Routing Rules'}
        subtitle={
          isRu
            ? 'Политики маршрутизации и фильтрации трафика по доменам, IP/CIDR, протоколам и базам Sing-box .srs'
            : 'Manage domain, IP, protocol, and Sing-box .srs rule-set policies'
        }
        icon={<Waypoints className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs tabular-nums">
            {stats.active}/{stats.total} {isRu ? 'активны' : 'active'}
          </Badge>
        }
        actions={
          <Button size="sm" onClick={openCreate}>
            <Plus className="h-4 w-4" />
            <span>{isRu ? 'Добавить правило' : 'Add Rule'}</span>
          </Button>
        }
      />

      {/* Routing KPI chip strip */}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded-xl border border-border/80 bg-background/60 px-3 py-2 text-[11px]">
        <span className="inline-flex items-center gap-1 font-mono tabular-nums">
          <Waypoints className="h-3 w-3 text-emerald-400" />
          <span className="font-bold text-foreground">{stats.active}</span>
          <span className="text-muted-foreground">/ {stats.total} {isRu ? 'активны' : 'active'}</span>
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <ShieldAlert className="h-3 w-3 text-red-400" />
          <span className="font-bold text-foreground">{stats.blockCount}</span>
          block
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Waypoints className="h-3 w-3 text-cyan-400" />
          <span className="font-bold text-foreground">{stats.directCount}</span>
          direct
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Sparkles className="h-3 w-3 text-amber-400" />
          <span className="font-bold text-foreground">{presets.length}</span>
          {isRu ? 'пресетов' : 'presets'}
        </span>
      </div>


      {presets.length > 0 && (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs uppercase tracking-wide text-muted-foreground">
              {isRu ? 'Быстрые пресеты (добавление в 1 клик)' : 'Quick Presets'}
            </CardTitle>
          </CardHeader>
          <CardContent className="flex flex-wrap gap-2">
            {presets.map((p) => (
              <Button
                key={p.id}
                size="sm"
                variant="outline"
                disabled={applyPresetMut.isPending}
                onClick={() => applyPresetMut.mutate(p.rule)}
              >
                <Sparkles className="h-3.5 w-3.5 text-primary" /> {p.name}
              </Button>
            ))}
          </CardContent>
        </Card>
      )}

      {isLoading ? (
        <Card className="p-6 text-center text-sm text-muted-foreground">
          {isRu ? 'Загрузка правил...' : 'Loading rules...'}
        </Card>
      ) : rules.length === 0 ? (
        <RackEmptyState
          icon={Waypoints}
          title={isRu ? 'Правила маршрутизации не заданы' : 'No routing rules defined'}
          description={
            isRu
              ? 'Используйте быстрые пресеты выше или создайте собственное правило маршрутизации.'
              : 'Use a quick preset above or create a custom routing rule.'
          }
          action={
            <Button size="sm" onClick={openCreate}>
              <Plus className="h-4 w-4" />
              <span>{isRu ? 'Добавить правило' : 'Add Rule'}</span>
            </Button>
          }
        />
      ) : (
        <div className="space-y-2.5">
          {rules.map((r, idx) => (
            <div
              key={r.id}
              className="node-blade relative flex flex-wrap items-center justify-between gap-3 overflow-hidden rounded-xl border border-border/80 p-2 pl-5 text-xs transition-colors hover:border-primary/45"
            >
              <div
                className={cn(
                  'absolute inset-y-0 left-0 w-1',
                  !r.enabled
                    ? 'bg-zinc-600'
                    : r.outboundTag === 'block'
                      ? 'bg-red-500'
                      : r.outboundTag === 'direct'
                        ? 'bg-emerald-400'
                        : 'bg-cyan-400',
                )}
                aria-hidden="true"
              />
              <div className="flex items-center gap-3 min-w-0 flex-1">
                <div className="flex items-center gap-0.5">
                  <Button size="icon" variant="ghost" className="h-7 w-7" onClick={() => moveRule(idx, -1)}>
                    <ArrowUp className="h-3.5 w-3.5" />
                  </Button>
                  <Button size="icon" variant="ghost" className="h-7 w-7" onClick={() => moveRule(idx, 1)}>
                    <ArrowDown className="h-3.5 w-3.5" />
                  </Button>
                </div>

                <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-primary/15 font-mono text-[11px] font-bold text-primary">
                  #{idx + 1}
                </span>

                <div className="min-w-0 flex-1 space-y-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-bold text-sm break-words min-w-0">{r.name}</span>
                    <Badge
                      variant={
                        r.outboundTag === 'block'
                          ? 'destructive'
                          : r.outboundTag === 'direct'
                            ? 'success'
                            : 'default'
                      }
                      className="font-mono text-[10px] uppercase"
                    >
                      {r.outboundTag}
                    </Badge>
                    <Badge variant={r.enabled ? 'success' : 'secondary'} className="text-[10px]">
                      {r.enabled ? (isRu ? 'Включено' : 'Enabled') : isRu ? 'Отключено' : 'Disabled'}
                    </Badge>
                  </div>
                  <div className="break-words font-mono text-[11px] text-muted-foreground min-w-0">
                    {[...(r.domain ?? []), ...(r.ip ?? []), ...(r.protocol ?? []), ...(r.ruleSet ?? [])].join(', ') ||
                      (isRu ? 'Весь трафик' : 'All traffic')}
                  </div>
                </div>
              </div>

              <div className="flex items-center gap-1">
                <Button size="icon" variant="ghost" onClick={() => toggleMut.mutate(r.id)}>
                  <Power className="h-4 w-4" />
                </Button>
                <Button size="icon" variant="ghost" onClick={() => openEdit(r)}>
                  <Edit className="h-4 w-4" />
                </Button>
                <Button size="icon" variant="ghost" onClick={() => deleteMut.mutate(r.id)}>
                  <Trash2 className="h-4 w-4 text-red-400" />
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}

      <Dialog open={modalOpen} onOpenChange={setModalOpen}>
        <DialogContent size="lg">
          <DialogHeader>
            <DialogTitle>
              {editing
                ? isRu
                  ? `Редактировать правило: ${editing.name}`
                  : `Edit Rule: ${editing.name}`
                : isRu
                  ? 'Создать правило маршрутизации'
                  : 'Create Routing Rule'}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="grid gap-2 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Название правила' : 'Rule Name'}</Label>
                <Input value={name} onChange={(e) => setName(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Действие (Outbound)' : 'Outbound Action'}</Label>
                <Select value={outboundTag} onValueChange={setOutboundTag}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="block">block</SelectItem>
                    <SelectItem value="direct">direct</SelectItem>
                    <SelectItem value="proxy">proxy</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Область нод' : 'Node Scope'}</Label>
                <Select value={nodeScope} onValueChange={setNodeScope}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">{isRu ? 'Все ноды' : 'All Nodes'}</SelectItem>
                    {nodes.map((n) => (
                      <SelectItem key={n.id} value={n.id}>
                        {n.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Порты' : 'Ports'}</Label>
                <Input placeholder="80,443" value={port} onChange={(e) => setPort(e.target.value)} />
              </div>
            </div>
            <div className="space-y-1.5">
              <Label>{isRu ? 'Домены (через запятую)' : 'Domains (comma-separated)'}</Label>
              <Input
                placeholder="geosite:category-ads-all, domain:example.com"
                value={domains}
                onChange={(e) => setDomains(e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <Label>{isRu ? 'IP / CIDR подсети (через запятую)' : 'IPs / CIDRs (comma-separated)'}</Label>
              <Input placeholder="geoip:private, 10.0.0.0/8" value={ips} onChange={(e) => setIps(e.target.value)} />
            </div>
            <div className="space-y-1.5">
              <Label>{isRu ? 'Протоколы (через запятую)' : 'Protocols (comma-separated)'}</Label>
              <Input placeholder="bittorrent, http, tls" value={protocols} onChange={(e) => setProtocols(e.target.value)} />
            </div>
            <div className="space-y-1.5">
              <Label>Sing-box .srs Rule Sets</Label>
              <Input
                placeholder="geosite-ads, geoip-ru"
                value={ruleSet}
                onChange={(e) => setRuleSet(e.target.value)}
              />
            </div>
            <div className="flex items-center justify-between rounded-lg border px-3 py-2">
              <Label>{isRu ? 'Включено' : 'Enabled'}</Label>
              <Switch checked={enabled} onCheckedChange={setEnabled} />
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
              {isRu ? 'Сохранить правило' : 'Save Rule'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
