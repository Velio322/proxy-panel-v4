import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Play, Plus, Sparkles, Trash2, Webhook } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
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
import { Checkbox } from '@/components/ui/checkbox';
import { RackEmptyState } from '@/components/ui/PagePrimitives';
import { TabsContent } from '@/components/ui/tabs';
import { webhooksApi, getApiErrorMessage } from '@/lib/api';
import { randomHex } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { WebhookItem } from '@/types';

const WEBHOOK_SCOPES = ['user', 'node', 'service'] as const;
const WEBHOOK_EVENTS = [
  'client.created',
  'client.updated',
  'client.deleted',
  'node.online',
  'node.offline',
  'traffic.limit',
] as const;

export function SettingsWebhooksTab() {
  const qc = useQueryClient();
  const { language } = useTranslation();
  const isRu = language === 'ru';
  const isSuperAdmin = useAuthStore((s) => s.user?.role === 'SUPER_ADMIN');
  // --- Webhooks CRUD
  const { data: webhooks = [] } = useQuery({
    queryKey: ['webhooks'],
    queryFn: () => webhooksApi.list(),
  });

  const [whModalOpen, setWhModalOpen] = useState(false);
  const [editingWh, setEditingWh] = useState<WebhookItem | null>(null);
  const [whName, setWhName] = useState('');
  const [whUrl, setWhUrl] = useState('');
  const [whSecret, setWhSecret] = useState('');
  const [whScopes, setWhScopes] = useState<string[]>(['user', 'node']);
  const [whEvents, setWhEvents] = useState<string[]>([]);
  const [whActive, setWhActive] = useState(true);

  const openCreateWh = () => {
    setEditingWh(null);
    setWhName('');
    setWhUrl('');
    setWhSecret(randomHex(24));
    setWhScopes(['user', 'node']);
    setWhEvents([]);
    setWhActive(true);
    setWhModalOpen(true);
  };

  const openEditWh = (wh: WebhookItem) => {
    setEditingWh(wh);
    setWhName(wh.name);
    setWhUrl(wh.url);
    setWhSecret('');
    setWhScopes(wh.scopes ?? []);
    setWhEvents(wh.events ?? []);
    setWhActive(wh.active);
    setWhModalOpen(true);
  };

  const saveWhMut = useMutation({
    mutationFn: async () => {
      const payload: Partial<WebhookItem> = {
        name: whName.trim(),
        url: whUrl.trim(),
        scopes: whScopes,
        events: whEvents,
        active: whActive,
        ...(whSecret.trim() ? { secret: whSecret.trim() } : {}),
      };
      if (editingWh) return webhooksApi.update(editingWh.id, payload);
      return webhooksApi.create(payload);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['webhooks'] });
      setWhModalOpen(false);
      toast({
        title: editingWh
          ? isRu
            ? 'Вебхук обновлён'
            : 'Webhook updated'
          : isRu
            ? 'Вебхук создан'
            : 'Webhook created',
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

  const testWhMut = useMutation({
    mutationFn: (id: string) => webhooksApi.test(id),
    onSuccess: (res) => {
      qc.invalidateQueries({ queryKey: ['webhooks'] });
      toast({
        title:
          res.success !== false
            ? isRu
              ? 'Тестовый вебхук доставлен'
              : 'Webhook test delivered'
            : isRu
              ? 'Вебхук вернул ошибку'
              : 'Webhook test returned error',
        description: res.status ? `HTTP ${res.status}` : res.error,
        variant: res.success !== false ? 'success' : 'destructive',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка теста' : 'Test failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const deleteWhMut = useMutation({
    mutationFn: (id: string) => webhooksApi.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['webhooks'] }),
  });


  return (
    <>
        <TabsContent value="webhooks" className="space-y-4">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <div>
                <CardTitle className="flex items-center gap-2">
                  <Webhook className="h-4 w-4 text-primary" />
                  <span>HMAC-SHA256 Webhooks</span>
                </CardTitle>
                <CardDescription>
                  {isRu ? (
                    <>
                      Запросы содержат заголовки <code>X-Signature</code> = hex(HMAC-SHA256(secret, &quot;
                      {'{timestamp}.{body}'}&quot;)), <code>X-Timestamp</code> и <code>X-Event</code>.
                    </>
                  ) : (
                    <>
                      Requests include <code>X-Signature</code> = hex(HMAC-SHA256(secret, &quot;
                      {'{timestamp}.{body}'}&quot;)), <code>X-Timestamp</code>, and <code>X-Event</code>.
                    </>
                  )}
                </CardDescription>
              </div>
              {isSuperAdmin && (
                <Button size="sm" onClick={openCreateWh}>
                  <Plus className="h-4 w-4" />
                  <span>{isRu ? 'Добавить вебхук' : 'Add Webhook'}</span>
                </Button>
              )}
            </CardHeader>
            <CardContent>
              {webhooks.length === 0 ? (
                <RackEmptyState
                  icon={Webhook}
                  title={isRu ? 'Вебхуки не настроены' : 'No webhooks configured'}
                  description={
                    isRu
                      ? 'Добавьте HTTP-эндпоинт для получения подписанных уведомлений о событиях клиентов и нод.'
                      : 'Add an HTTP endpoint to receive HMAC-signed notifications for user and node events.'
                  }
                  action={
                    isSuperAdmin ? (
                      <Button size="sm" onClick={openCreateWh}>
                        <Plus className="h-4 w-4" />
                        <span>{isRu ? 'Добавить вебхук' : 'Add Webhook'}</span>
                      </Button>
                    ) : undefined
                  }
                />
              ) : (
                <div className="space-y-2.5">
                  {webhooks.map((wh) => (
                    <div
                      key={wh.id}
                      className="rack-inset flex flex-wrap items-center justify-between gap-3 rounded-xl p-2 text-xs"
                    >
                      <div className="space-y-1">
                        <div className="flex items-center gap-2">
                          <span className="font-semibold text-sm">{wh.name}</span>
                          <Badge variant={wh.active ? 'success' : 'secondary'}>
                            {wh.active ? (isRu ? 'Активен' : 'Active') : isRu ? 'Пауза' : 'Paused'}
                          </Badge>
                          {wh.lastStatus && (
                            <Badge variant={wh.lastStatus < 400 ? 'outline' : 'destructive'}>
                              HTTP {wh.lastStatus}
                            </Badge>
                          )}
                        </div>
                        <div className="font-mono text-muted-foreground">{wh.url}</div>
                        <div className="flex flex-wrap gap-1">
                          {(wh.scopes ?? []).map((s) => (
                            <Badge key={s} variant="outline" className="text-[10px]">
                              scope:{s}
                            </Badge>
                          ))}
                          {(wh.events ?? []).map((ev) => (
                            <Badge key={ev} variant="secondary" className="text-[10px]">
                              {ev}
                            </Badge>
                          ))}
                        </div>
                      </div>
                      <div className="flex items-center gap-1">
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => testWhMut.mutate(wh.id)}
                          loading={testWhMut.isPending}
                        >
                          <Play className="h-3.5 w-3.5" /> {isRu ? 'Тест' : 'Test'}
                        </Button>
                        {isSuperAdmin && (
                          <>
                            <Button size="sm" variant="ghost" onClick={() => openEditWh(wh)}>
                              {isRu ? 'Изменить' : 'Edit'}
                            </Button>
                            <Button size="icon" variant="ghost" onClick={() => deleteWhMut.mutate(wh.id)}>
                              <Trash2 className="h-4 w-4 text-red-400" />
                            </Button>
                          </>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>
      <Dialog open={whModalOpen} onOpenChange={setWhModalOpen}>
        <DialogContent size="lg">
          <DialogHeader>
            <DialogTitle>
              {editingWh
                ? isRu
                  ? `Редактировать вебхук: ${editingWh.name}`
                  : `Edit Webhook: ${editingWh.name}`
                : isRu
                  ? 'Создать вебхук'
                  : 'Create Webhook'}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label>{isRu ? 'Название' : 'Name'}</Label>
              <Input value={whName} onChange={(e) => setWhName(e.target.value)} />
            </div>
            <div className="space-y-1.5">
              <Label>Endpoint URL</Label>
              <Input value={whUrl} onChange={(e) => setWhUrl(e.target.value)} placeholder="https://example.com/hook" />
            </div>
            <div className="space-y-1.5">
              <div className="flex items-center justify-between">
                <Label>HMAC Secret</Label>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="h-6 px-2 text-xs"
                  onClick={() => setWhSecret(randomHex(24))}
                >
                  <Sparkles className="h-3 w-3" /> {isRu ? 'Сгенерировать' : 'Generate'}
                </Button>
              </div>
              <Input
                value={whSecret}
                onChange={(e) => setWhSecret(e.target.value)}
                placeholder={
                  editingWh
                    ? isRu
                      ? 'Оставьте пустым для сохранения текущего'
                      : 'Leave blank to keep existing'
                    : 'CSPRNG HMAC secret'
                }
                className="font-mono text-xs"
              />
            </div>
            <div className="space-y-1.5">
              <Label>Scopes</Label>
              <div className="flex flex-wrap gap-2">
                {WEBHOOK_SCOPES.map((sc) => (
                  <label key={sc} className="flex cursor-pointer items-center gap-1.5 rounded border px-2.5 py-1 text-xs">
                    <Checkbox
                      checked={whScopes.includes(sc)}
                      onCheckedChange={() =>
                        setWhScopes((prev) =>
                          prev.includes(sc) ? prev.filter((x) => x !== sc) : [...prev, sc],
                        )
                      }
                    />
                    <span>{sc}</span>
                  </label>
                ))}
              </div>
            </div>
            <div className="space-y-1.5">
              <Label>{isRu ? 'События (фильтр)' : 'Events (optional filter)'}</Label>
              <div className="flex flex-wrap gap-1.5">
                {WEBHOOK_EVENTS.map((ev) => (
                  <label key={ev} className="flex cursor-pointer items-center gap-1.5 rounded border px-2 py-1 text-[11px]">
                    <Checkbox
                      checked={whEvents.includes(ev)}
                      onCheckedChange={() =>
                        setWhEvents((prev) =>
                          prev.includes(ev) ? prev.filter((x) => x !== ev) : [...prev, ev],
                        )
                      }
                    />
                    <span>{ev}</span>
                  </label>
                ))}
              </div>
            </div>
            <div className="flex items-center justify-between rounded-lg border px-3 py-2">
              <Label>{isRu ? 'Активен' : 'Active'}</Label>
              <Switch checked={whActive} onCheckedChange={setWhActive} />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setWhModalOpen(false)}>
              {isRu ? 'Отмена' : 'Cancel'}
            </Button>
            <Button
              onClick={() => saveWhMut.mutate()}
              loading={saveWhMut.isPending}
              disabled={!whName.trim() || !whUrl.trim()}
            >
              {isRu ? 'Сохранить вебхук' : 'Save Webhook'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
