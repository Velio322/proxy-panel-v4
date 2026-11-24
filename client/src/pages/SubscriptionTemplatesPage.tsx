import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Edit, FileCode2, Plus, Shield, Trash2 } from 'lucide-react';
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
import { PageHero, RackEmptyState } from '@/components/ui/PagePrimitives';
import { JsonEditor } from '@/components/JsonEditor';
import { getApiErrorMessage, templatesApi } from '@/lib/api';
import { cn } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { SubTemplateType, SubscriptionTemplate } from '@/types';

const TEMPLATE_TYPES: SubTemplateType[] = [
  'XRAY_JSON',
  'XRAY_BASE64',
  'MIHOMO',
  'CLASH',
  'STASH',
  'SINGBOX',
];

const VARIABLES = ['{{proxies}}', '{{links}}', '{{username}}', '{{subUrl}}'] as const;

export function SubscriptionTemplatesPage() {
  const qc = useQueryClient();
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<SubscriptionTemplate | null>(null);

  const [name, setName] = useState('');
  const [templateType, setTemplateType] = useState<SubTemplateType>('MIHOMO');
  const [content, setContent] = useState('proxies:\n  {{proxies}}');
  const [isDefault, setIsDefault] = useState(false);
  const [editorErrors, setEditorErrors] = useState<string[]>([]);

  const { data: templates = [], isLoading } = useQuery({
    queryKey: ['templates'],
    queryFn: () => templatesApi.list(),
  });

  const openCreate = () => {
    setEditing(null);
    setName('');
    setTemplateType('MIHOMO');
    setContent('proxies:\n  # {{proxies}}');
    setIsDefault(false);
    setModalOpen(true);
  };

  const openEdit = (tmpl: SubscriptionTemplate) => {
    setEditing(tmpl);
    setName(tmpl.name);
    setTemplateType(tmpl.templateType);
    setContent(tmpl.content);
    setIsDefault(tmpl.isDefault);
    setModalOpen(true);
  };

  const editorLang =
    templateType === 'XRAY_JSON' || templateType === 'SINGBOX'
      ? 'json'
      : templateType === 'XRAY_BASE64'
        ? 'plaintext'
        : 'yaml';

  const saveMut = useMutation({
    mutationFn: async () => {
      const payload: Partial<SubscriptionTemplate> = {
        name: name.trim(),
        templateType,
        content,
        isDefault,
      };
      if (editing) return templatesApi.update(editing.id, payload);
      return templatesApi.create(payload);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['templates'] });
      setModalOpen(false);
      toast({
        title: editing
          ? isRu
            ? 'Шаблон обновлён'
            : 'Template updated'
          : isRu
            ? 'Шаблон создан'
            : 'Template created',
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
    mutationFn: (id: string) => templatesApi.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['templates'] }),
  });

  const defaultCount = templates.filter((t) => t.isDefault).length;
  const distinctTypes = new Set(templates.map((t) => t.templateType)).size;

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Шаблоны подписок' : 'Subscription Templates'}
        subtitle={
          isRu
            ? 'Кастомизация конфигураций Xray, Sing-box, Mihomo, Clash и Stash для клиентских приложений'
            : 'Customize Xray, Sing-box, Mihomo, Clash, and Stash subscription payloads.'
        }
        icon={<FileCode2 className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs tabular-nums">
            {templates.length} {isRu ? 'шаблонов' : 'templates'}
          </Badge>
        }
        actions={
          <>
            <Button variant="outline" size="sm" asChild>
              <Link to="/srr">
                <Shield className="h-4 w-4" />
                <span>{isRu ? 'Правила SRR и age-шифрование' : 'SRR Rules & age Encryption'}</span>
              </Link>
            </Button>
            <Button size="sm" onClick={openCreate}>
              <Plus className="h-4 w-4" />
              <span>{isRu ? 'Создать шаблон' : 'Create Template'}</span>
            </Button>
          </>
        }
      />

      {/* 3-Card KPI Ribbon */}
      <div className="grid gap-3 sm:grid-cols-3">
        <Card className="p-3 space-y-1">
          <div className="text-xs text-muted-foreground">{isRu ? 'Всего шаблонов' : 'Total Templates'}</div>
          <div className="font-mono text-2xl font-bold tabular-nums">{templates.length}</div>
        </Card>
        <Card className="p-3 space-y-1">
          <div className="text-xs text-muted-foreground">{isRu ? 'Шаблоны по умолчанию' : 'Default Profiles'}</div>
          <div className="font-mono text-2xl font-bold tabular-nums">{defaultCount}</div>
        </Card>
        <Card className="p-3 space-y-1">
          <div className="text-xs text-muted-foreground">{isRu ? 'Покрыто форматов' : 'Formats Covered'}</div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {distinctTypes} / {TEMPLATE_TYPES.length}
          </div>
        </Card>
      </div>

      {isLoading ? (
        <Card className="p-6 text-center text-sm text-muted-foreground">
          {isRu ? 'Загрузка шаблонов...' : 'Loading templates...'}
        </Card>
      ) : templates.length === 0 ? (
        <RackEmptyState
          icon={FileCode2}
          title={isRu ? 'Шаблоны подписок не созданы' : 'No templates configured'}
          description={
            isRu
              ? 'Создайте кастомный YAML/JSON шаблон для Mihomo, Clash, Sing-box или Xray.'
              : 'Create a custom YAML/JSON template for Mihomo, Clash, Sing-box, or Xray.'
          }
          action={
            <Button size="sm" onClick={openCreate}>
              <Plus className="h-4 w-4" />
              <span>{isRu ? 'Создать шаблон' : 'Create Template'}</span>
            </Button>
          }
        />
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {templates.map((tmpl) => (
            <div
              key={tmpl.id}
              className="node-blade relative flex flex-col justify-between overflow-hidden rounded-xl border border-border/80 p-2 pl-3.5 transition-colors hover:border-primary/45"
            >
              <div
                className={cn(
                  'absolute inset-y-0 left-0 w-1',
                  tmpl.isDefault ? 'bg-emerald-400' : 'bg-cyan-400',
                )}
                aria-hidden="true"
              />
              <div className="space-y-3">
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2 font-bold text-sm break-words min-w-0">
                    <FileCode2 className="h-4 w-4 text-primary shrink-0" />
                    <span className="break-words min-w-0">{tmpl.name}</span>
                  </div>
                  {tmpl.isDefault && (
                    <Badge variant="success">{isRu ? 'По умолч.' : 'Default'}</Badge>
                  )}
                </div>
                <Badge variant="outline" className="font-mono text-[10px]">
                  {tmpl.templateType}
                </Badge>
                <pre className="rack-inset max-h-28 overflow-hidden rounded-lg p-2 font-mono text-[11px] text-muted-foreground">
                  {tmpl.content}
                </pre>
              </div>
              <div className="mt-3 flex justify-end gap-1 border-t border-border/60 pt-2.5">
                <Button size="sm" variant="outline" className="h-7 text-xs" onClick={() => openEdit(tmpl)}>
                  <Edit className="h-3.5 w-3.5" /> {isRu ? 'Изменить' : 'Edit'}
                </Button>
                <Button size="sm" variant="ghost" className="h-7 px-2" onClick={() => deleteMut.mutate(tmpl.id)}>
                  <Trash2 className="h-3.5 w-3.5 text-red-400" />
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}

      <Dialog open={modalOpen} onOpenChange={setModalOpen}>
        <DialogContent size="2xl">
          <DialogHeader>
            <DialogTitle>
              {editing
                ? isRu
                  ? `Редактировать шаблон: ${editing.name}`
                  : `Edit Template: ${editing.name}`
                : isRu
                  ? 'Создать шаблон подписки'
                  : 'Create Subscription Template'}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="grid gap-2 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Название шаблона' : 'Template Name'}</Label>
                <Input value={name} onChange={(e) => setName(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Формат' : 'Format'}</Label>
                <Select
                  value={templateType}
                  onValueChange={(v) => setTemplateType(v as SubTemplateType)}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {TEMPLATE_TYPES.map((tt) => (
                      <SelectItem key={tt} value={tt}>
                        {tt}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-1.5">
              <span className="text-xs text-muted-foreground">
                {isRu ? 'Вставить переменную:' : 'Insert variable:'}
              </span>
              {VARIABLES.map((v) => (
                <Button
                  key={v}
                  type="button"
                  size="sm"
                  variant="outline"
                  className="h-6 px-2 font-mono text-[11px]"
                  onClick={() => setContent((prev) => `${prev}\n${v}`)}
                >
                  {v}
                </Button>
              ))}
            </div>

            <JsonEditor
              value={content}
              onChange={setContent}
              language={editorLang}
              schemaKind="subscription-template"
              onValidationChange={setEditorErrors}
              height="260px"
            />

            <div className="flex items-center justify-between rounded-lg border px-3 py-2">
              <Label>{isRu ? 'Использовать по умолчанию для формата' : 'Set as Default for Format'}</Label>
              <Switch checked={isDefault} onCheckedChange={setIsDefault} />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setModalOpen(false)}>
              {isRu ? 'Отмена' : 'Cancel'}
            </Button>
            <Button
              onClick={() => saveMut.mutate()}
              loading={saveMut.isPending}
              disabled={!name.trim() || editorErrors.length > 0}
            >
              {isRu ? 'Сохранить шаблон' : 'Save Template'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
