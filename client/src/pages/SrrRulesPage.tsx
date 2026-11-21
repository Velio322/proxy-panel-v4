import { useMemo, useState } from 'react';
import { Link } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { ArrowDown, ArrowLeft, ArrowUp, Edit, Plus, Power, Shield, Trash2 } from 'lucide-react';
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
import { getApiErrorMessage, srrApi, templatesApi } from '@/lib/api';
import { cn } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { SrrAction, SrrRule } from '@/types';

const SRR_ACTIONS: SrrAction[] = [
  'ALLOW',
  'USE_TEMPLATE',
  'RESPOND_WITH_REMARKS',
  'ENCRYPT_AGE',
  'BLOCK_404',
  'BLOCK_451',
  'SOCKET_DROP',
];

export function isValidAgePublicKey(key: string): boolean {
  return /^age1[0-9a-z]{50,}$/i.test(key.trim());
}

export function SrrRulesPage() {
  const qc = useQueryClient();
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<SrrRule | null>(null);

  const [name, setName] = useState('');
  const [enabled, setEnabled] = useState(true);
  const [priority, setPriority] = useState('1');
  const [matchHeader, setMatchHeader] = useState('User-Agent');
  const [matchPattern, setMatchPattern] = useState('');
  const [action, setAction] = useState<SrrAction>('ALLOW');
  const [templateId, setTemplateId] = useState('none');
  const [customRemarks, setCustomRemarks] = useState('');
  const [agePublicKey, setAgePublicKey] = useState('');

  // Simulator
  const [testHeaderName, setTestHeaderName] = useState('User-Agent');
  const [testHeaderValue, setTestHeaderValue] = useState('Happ/2.0');

  const { data: rules = [], isLoading } = useQuery({
    queryKey: ['srr-rules'],
    queryFn: () => srrApi.list(),
  });

  const { data: templates = [] } = useQuery({
    queryKey: ['templates'],
    queryFn: () => templatesApi.list(),
  });

  const openCreate = () => {
    setEditing(null);
    setName('');
    setEnabled(true);
    setPriority(String(rules.length + 1));
    setMatchHeader('User-Agent');
    setMatchPattern('');
    setAction('ALLOW');
    setTemplateId('none');
    setCustomRemarks('');
    setAgePublicKey('');
    setModalOpen(true);
  };

  const openEdit = (r: SrrRule) => {
    setEditing(r);
    setName(r.name);
    setEnabled(r.enabled);
    setPriority(String(r.priority));
    setMatchHeader(r.matchHeader);
    setMatchPattern(r.matchPattern);
    setAction(r.action);
    setTemplateId(r.templateId || 'none');
    setCustomRemarks(r.customRemarks ?? '');
    setAgePublicKey(r.agePublicKey ?? '');
    setModalOpen(true);
  };

  const ageInvalid =
    action === 'ENCRYPT_AGE' && (!agePublicKey.trim() || !isValidAgePublicKey(agePublicKey));

  const saveMut = useMutation({
    mutationFn: async () => {
      if (ageInvalid) throw new Error('Valid age1... recipient public key required');
      const payload: Partial<SrrRule> = {
        name: name.trim(),
        enabled,
        priority: Number(priority) || 1,
        matchHeader: matchHeader.trim(),
        matchPattern: matchPattern.trim(),
        action,
        templateId: templateId !== 'none' ? templateId : null,
        customRemarks: customRemarks.trim() || null,
        agePublicKey: agePublicKey.trim() || null,
      };
      if (editing) return srrApi.update(editing.id, payload);
      return srrApi.create(payload);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['srr-rules'] });
      setModalOpen(false);
      toast({
        title: editing
          ? isRu
            ? 'Правило SRR обновлено'
            : 'SRR rule updated'
          : isRu
            ? 'Правило SRR создано'
            : 'SRR rule created',
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
    mutationFn: (id: string) => srrApi.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['srr-rules'] }),
  });

  const toggleMut = useMutation({
    mutationFn: (rule: SrrRule) => srrApi.update(rule.id, { enabled: !rule.enabled }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['srr-rules'] }),
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка переключения' : 'Toggle failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const reorderMut = useMutation({
    mutationFn: (ids: string[]) => srrApi.reorder(ids),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['srr-rules'] }),
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка сортировки' : 'Reorder failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const sortedRules = useMemo(
    () => [...rules].sort((a, b) => a.priority - b.priority),
    [rules],
  );

  const moveRule = (index: number, delta: -1 | 1) => {
    const target = index + delta;
    if (target < 0 || target >= sortedRules.length) return;
    const copy = [...sortedRules];
    const [moved] = copy.splice(index, 1);
    copy.splice(target, 0, moved);
    reorderMut.mutate(copy.map((r) => r.id));
  };

  const matchedRule = useMemo(() => {
    const sorted = [...rules].filter((r) => r.enabled).sort((a, b) => a.priority - b.priority);
    return sorted.find((r) => {
      if (r.matchHeader.toLowerCase() !== testHeaderName.trim().toLowerCase()) return false;
      try {
        return new RegExp(r.matchPattern, 'i').test(testHeaderValue);
      } catch {
        return false;
      }
    });
  }, [rules, testHeaderName, testHeaderValue]);

  const activeCount = rules.filter((r) => r.enabled).length;
  const blockCount = rules.filter((r) => r.action.startsWith('BLOCK') || r.action === 'SOCKET_DROP').length;

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Правила запросов подписок (SRR)' : 'Subscription Request Rules (SRR)'}
        subtitle={
          isRu
            ? 'Фильтрация заголовков User-Agent, x-hwid и Accept для выбора шаблона, блокировки ботов или шифрования age'
            : 'Match User-Agent, x-hwid, or Accept headers to assign templates, block bots, or encrypt with age.'
        }
        icon={<Shield className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs tabular-nums">
            {activeCount}/{rules.length}
          </Badge>
        }
        actions={
          <>
            <Button variant="outline" size="sm" asChild>
              <Link to="/templates">
                <ArrowLeft className="h-4 w-4" />
                <span>{isRu ? 'К шаблонам' : 'Templates'}</span>
              </Link>
            </Button>
            <Button size="sm" onClick={openCreate}>
              <Plus className="h-4 w-4" />
              <span>{isRu ? 'Добавить правило SRR' : 'Add SRR Rule'}</span>
            </Button>
          </>
        }
      />

      {/* 3-Card KPI Ribbon */}
      <div className="grid gap-4 sm:grid-cols-3">
        <Card className="p-4 space-y-1">
          <div className="text-xs text-muted-foreground">{isRu ? 'Активные правила' : 'Active Rules'}</div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {activeCount} / {rules.length}
          </div>
        </Card>
        <Card className="p-4 space-y-1">
          <div className="text-xs text-muted-foreground">{isRu ? 'Правила блокировки' : 'Block / Drop Rules'}</div>
          <div className="font-mono text-2xl font-bold tabular-nums">{blockCount}</div>
        </Card>
        <Card className="p-4 space-y-1">
          <div className="text-xs text-muted-foreground">{isRu ? 'Доступно шаблонов' : 'Linked Templates'}</div>
          <div className="font-mono text-2xl font-bold tabular-nums">{templates.length}</div>
        </Card>
      </div>

      {/* Header Simulator */}
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-sm">
            {isRu ? 'Симулятор HTTP-заголовков' : 'Header Simulator'}
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-wrap items-center gap-3 text-xs">
          <Input
            value={testHeaderName}
            onChange={(e) => setTestHeaderName(e.target.value)}
            placeholder={isRu ? 'Имя заголовка' : 'Header name'}
            className="w-44 font-mono"
          />
          <Input
            value={testHeaderValue}
            onChange={(e) => setTestHeaderValue(e.target.value)}
            placeholder={isRu ? 'Значение заголовка' : 'Header value'}
            className="flex-1 min-w-[200px] font-mono"
          />
          <div>
            {isRu ? 'Результат: ' : 'Matched: '}
            {matchedRule ? (
              <Badge variant="success">
                #{matchedRule.priority} {matchedRule.name} → {matchedRule.action}
              </Badge>
            ) : (
              <Badge variant="secondary">
                {isRu ? 'По умолчанию (совпадений нет)' : 'Default (no rule matched)'}
              </Badge>
            )}
          </div>
        </CardContent>
      </Card>

      {isLoading ? (
        <Card className="p-8 text-center text-sm text-muted-foreground">
          {isRu ? 'Загрузка правил...' : 'Loading rules...'}
        </Card>
      ) : rules.length === 0 ? (
        <RackEmptyState
          icon={Shield}
          title={isRu ? 'Правила SRR не настроены' : 'No SRR rules configured'}
          description={
            isRu
              ? 'Создайте правило SRR для маршрутизации клиентских приложений по разным шаблонам или защиты ссылки подписки.'
              : 'Add an SRR rule to route client apps to specific templates or block unwanted crawlers.'
          }
          action={
            <Button size="sm" onClick={openCreate}>
              <Plus className="h-4 w-4" />
              <span>{isRu ? 'Добавить правило SRR' : 'Add SRR Rule'}</span>
            </Button>
          }
        />
      ) : (
        <div className="space-y-2.5">
          {sortedRules.map((r, idx) => (
            <div
              key={r.id}
              className="node-blade relative flex flex-wrap items-center justify-between gap-2 overflow-hidden rounded-xl border border-border/80 p-3.5 pl-5 text-xs transition-colors hover:border-primary/45"
            >
              <div
                className={cn(
                  'absolute inset-y-0 left-0 w-1',
                  !r.enabled
                    ? 'bg-zinc-600'
                    : r.action.startsWith('BLOCK') || r.action === 'SOCKET_DROP'
                      ? 'bg-red-500'
                      : 'bg-emerald-400',
                )}
                aria-hidden="true"
              />
              <div className="flex items-start gap-2.5">
                <div className="flex flex-col gap-0.5 pt-0.5">
                  <Button
                    size="icon"
                    variant="ghost"
                    className="h-5 w-5"
                    disabled={idx === 0 || reorderMut.isPending}
                    onClick={() => moveRule(idx, -1)}
                    aria-label={`Move ${r.name} up`}
                  >
                    <ArrowUp className="h-3 w-3" />
                  </Button>
                  <Button
                    size="icon"
                    variant="ghost"
                    className="h-5 w-5"
                    disabled={idx === sortedRules.length - 1 || reorderMut.isPending}
                    onClick={() => moveRule(idx, 1)}
                    aria-label={`Move ${r.name} down`}
                  >
                    <ArrowDown className="h-3 w-3" />
                  </Button>
                </div>
                <div className="space-y-0.5">
                  <div className="flex flex-wrap items-center gap-2">
                    <Shield className="h-3.5 w-3.5 text-primary" />
                    <span className="font-semibold text-sm tabular-nums">
                      #{r.priority} {r.name}
                    </span>
                    <Badge variant="outline" className="font-mono text-[10px]">
                      {r.action}
                    </Badge>
                    <Badge variant={r.enabled ? 'success' : 'secondary'}>
                      {r.enabled ? (isRu ? 'Включено' : 'Enabled') : isRu ? 'Отключено' : 'Disabled'}
                    </Badge>
                  </div>
                  <div className="font-mono text-muted-foreground">
                    {r.matchHeader} =~ /{r.matchPattern}/
                    {r.agePublicKey && ` · age:${r.agePublicKey.slice(0, 14)}...`}
                  </div>
                </div>
              </div>
              <div className="flex gap-1">
                <Button
                  size="icon"
                  variant="ghost"
                  onClick={() => toggleMut.mutate(r)}
                  aria-label={r.enabled ? `Disable ${r.name}` : `Enable ${r.name}`}
                >
                  <Power className={`h-4 w-4 ${r.enabled ? 'text-emerald-500' : 'text-muted-foreground'}`} />
                </Button>
                <Button size="icon" variant="ghost" onClick={() => openEdit(r)}>
                  <Edit className="h-4 w-4" />
                </Button>
                <Button size="icon" variant="ghost" onClick={() => deleteMut.mutate(r.id)}>
                  <Trash2 className="h-4 w-4 text-red-500 dark:text-red-400" />
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}

      <Dialog open={modalOpen} onOpenChange={setModalOpen}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>
              {editing
                ? isRu
                  ? `Редактировать правило SRR: ${editing.name}`
                  : `Edit SRR Rule: ${editing.name}`
                : isRu
                  ? 'Создать правило SRR'
                  : 'Create SRR Rule'}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Название правила' : 'Rule Name'}</Label>
                <Input value={name} onChange={(e) => setName(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Приоритет' : 'Priority'}</Label>
                <Input type="number" min={1} value={priority} onChange={(e) => setPriority(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Заголовок (Header)' : 'Match Header'}</Label>
                <Input value={matchHeader} onChange={(e) => setMatchHeader(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Regex-шаблон' : 'Match Regex Pattern'}</Label>
                <Input value={matchPattern} onChange={(e) => setMatchPattern(e.target.value)} placeholder="Happ|FlClash" />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Действие' : 'Action'}</Label>
                <Select value={action} onValueChange={(v) => setAction(v as SrrAction)}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {SRR_ACTIONS.map((a) => (
                      <SelectItem key={a} value={a}>
                        {a}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              {action === 'USE_TEMPLATE' && (
                <div className="space-y-1.5">
                  <Label>{isRu ? 'Шаблон' : 'Template'}</Label>
                  <Select value={templateId} onValueChange={setTemplateId}>
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="none">{isRu ? 'Выберите шаблон...' : 'Select template...'}</SelectItem>
                      {templates.map((t) => (
                        <SelectItem key={t.id} value={t.id}>
                          {t.name} ({t.templateType})
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              )}
            </div>

            {action === 'ENCRYPT_AGE' && (
              <div className="space-y-1.5">
                <Label>age Public Key (age1...)</Label>
                <Input
                  value={agePublicKey}
                  onChange={(e) => setAgePublicKey(e.target.value)}
                  placeholder="age1..."
                  className="font-mono text-xs"
                />
                {ageInvalid && (
                  <p className="text-xs text-red-400">
                    {isRu ? 'Требуется корректный публичный ключ Bech32 age1...' : 'Must be a valid Bech32 age1... public key'}
                  </p>
                )}
              </div>
            )}

            {action === 'RESPOND_WITH_REMARKS' && (
              <div className="space-y-1.5">
                <Label>{isRu ? 'Текст уведомления (Remarks)' : 'Custom Remarks'}</Label>
                <Input value={customRemarks} onChange={(e) => setCustomRemarks(e.target.value)} />
              </div>
            )}

            <div className="flex items-center justify-between rounded-lg border px-3 py-2">
              <Label>{isRu ? 'Включено' : 'Enabled'}</Label>
              <Switch checked={enabled} onCheckedChange={setEnabled} />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setModalOpen(false)}>
              {isRu ? 'Отмена' : 'Cancel'}
            </Button>
            <Button
              onClick={() => saveMut.mutate()}
              loading={saveMut.isPending}
              disabled={!name.trim() || !matchPattern.trim() || ageInvalid}
            >
              {isRu ? 'Сохранить правило' : 'Save Rule'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
