import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Check, Copy, Key, Plus, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import { Checkbox } from '@/components/ui/checkbox';
import { RackEmptyState } from '@/components/ui/PagePrimitives';
import { TabsContent } from '@/components/ui/tabs';
import { useUiStore } from '@/lib/store';
import { usersApi, getApiErrorMessage } from '@/lib/api';
import { formatDateTime } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import { useCopyFeedback } from '@/hooks/use-copy-feedback';

const API_KEY_PERMISSIONS = ['read', 'write', 'clients:manage', 'nodes:manage'] as const;

export function SettingsApiKeysTab() {
  const qc = useQueryClient();
  const { language } = useTranslation();
  const isRu = language === 'ru';
  const user = useAuthStore((s) => s.user);
  const timezone = useUiStore((s) => s.timezone);
  const isSuperAdmin = user?.role === 'SUPER_ADMIN';
  const isAdmin = user?.role === 'SUPER_ADMIN' || user?.role === 'ADMIN';

  // Real API Keys CRUD
  const [selectedKeyUserId, setSelectedKeyUserId] = useState<string>('');
  const effectiveKeyUserId = selectedKeyUserId || user?.id || '';

  const { data: usersRes } = useQuery({
    queryKey: ['users-select'],
    queryFn: () => usersApi.list({ limit: 100 }),
    enabled: isSuperAdmin,
  });

  const { data: apiKeys = [], error: apiKeysError } = useQuery({
    queryKey: ['api-keys', effectiveKeyUserId],
    queryFn: () => usersApi.listApiKeys(effectiveKeyUserId),
    enabled: isAdmin && Boolean(effectiveKeyUserId),
    retry: false,
  });

  const [newKeyModalOpen, setNewKeyModalOpen] = useState(false);
  const [newKeyName, setNewKeyName] = useState('');
  const [newKeyPerms, setNewKeyPerms] = useState<string[]>(['read']);
  const [newKeyExpiry, setNewKeyExpiry] = useState('');
  const [createdKeyToken, setCreatedKeyToken] = useState<string | null>(null);
  const [copiedToken, copyToken] = useCopyFeedback();

  const createApiKeyMut = useMutation({
    mutationFn: () =>
      usersApi.createApiKey(effectiveKeyUserId, {
        name: newKeyName.trim(),
        permissions: newKeyPerms,
        expiresAt: newKeyExpiry ? new Date(`${newKeyExpiry}T23:59:59Z`).toISOString() : null,
      }),
    onSuccess: (created) => {
      qc.invalidateQueries({ queryKey: ['api-keys', effectiveKeyUserId] });
      setNewKeyModalOpen(false);
      setNewKeyName('');
      if (created.key) setCreatedKeyToken(created.key);
      toast({ title: isRu ? 'API-ключ создан' : 'API key created', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка' : 'Failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const deleteApiKeyMut = useMutation({
    mutationFn: (keyId: string) => usersApi.deleteApiKey(effectiveKeyUserId, keyId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['api-keys', effectiveKeyUserId] });
      toast({ title: isRu ? 'API-ключ отозван' : 'API key revoked', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка отзыва' : 'Revoke failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });


  return (
    <>
        <TabsContent value="api-keys" className="space-y-4">
          <Card>
            <CardHeader className="flex flex-row flex-wrap items-center justify-between gap-3">
              <div>
                <CardTitle className="flex items-center gap-2">
                  <Key className="h-4 w-4 text-primary" />
                  <span>{isRu ? 'API-ключи' : 'API Keys'}</span>
                </CardTitle>
                <CardDescription>
                  {isRu
                    ? 'Токены программного доступа к API (хранятся на сервере в виде SHA-256 хеша).'
                    : 'Programmatic access tokens stored hashed via SHA-256 on the server.'}
                </CardDescription>
              </div>
              <div className="flex items-center gap-2">
                {isSuperAdmin && usersRes?.data && (
                  <div className="w-44">
                    <Select
                      value={effectiveKeyUserId || 'none'}
                      onValueChange={(v) => v !== 'none' && setSelectedKeyUserId(v)}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {usersRes.data.map((u) => (
                          <SelectItem key={u.id} value={u.id}>
                            {u.username}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                )}
                <Button size="sm" onClick={() => setNewKeyModalOpen(true)}>
                  <Plus className="h-4 w-4" />
                  <span>{isRu ? 'Создать API-ключ' : 'Create API Key'}</span>
                </Button>
              </div>
            </CardHeader>
            <CardContent>
              {apiKeysError ? (
                <div className="text-xs text-red-400">
                  {getApiErrorMessage(apiKeysError, isRu ? 'Доступ запрещён' : 'Permission denied')}
                </div>
              ) : apiKeys.length === 0 ? (
                <RackEmptyState
                  icon={Key}
                  title={isRu ? 'API-ключи не созданы' : 'No API keys created'}
                  description={
                    isRu
                      ? 'Создайте API-ключ для интеграции внешних биллингов, Telegram-ботов или скриптов.'
                      : 'Create an API key to integrate external billing systems, bots, or automation scripts.'
                  }
                  action={
                    <Button size="sm" onClick={() => setNewKeyModalOpen(true)}>
                      <Plus className="h-4 w-4" />
                      <span>{isRu ? 'Создать API-ключ' : 'Create API Key'}</span>
                    </Button>
                  }
                />
              ) : (
                <div className="space-y-2">
                  {apiKeys.map((k) => (
                    <div
                      key={k.id}
                      className="rack-inset flex items-center justify-between rounded-xl px-2.5 py-1.5 text-xs"
                    >
                      <div className="space-y-1">
                        <div className="font-semibold text-sm">{k.name}</div>
                        <div className="flex flex-wrap gap-1">
                          {(k.permissions ?? []).map((p) => (
                            <Badge key={p} variant="outline" className="text-[10px] font-mono">
                              {p}
                            </Badge>
                          ))}
                        </div>
                        <div className="text-muted-foreground font-mono text-[11px]">
                          {isRu ? 'Создан: ' : 'Created '}
                          {formatDateTime(k.createdAt, timezone)}
                          {k.lastUsedAt &&
                            ` · ${isRu ? 'Использован: ' : 'Last used '}${formatDateTime(k.lastUsedAt, timezone)}`}
                        </div>
                      </div>
                      <Button
                        size="icon"
                        variant="ghost"
                        aria-label={`Revoke ${k.name}`}
                        onClick={() => deleteApiKeyMut.mutate(k.id)}
                      >
                        <Trash2 className="h-4 w-4 text-red-400" />
                      </Button>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        {/* Webhooks Tab */}

      {/* Create API Key Modal */}
      <Dialog open={newKeyModalOpen} onOpenChange={setNewKeyModalOpen}>
        <DialogContent size="md">
          <DialogHeader>
            <DialogTitle>{isRu ? 'Создать API-ключ' : 'Create API Key'}</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label>{isRu ? 'Название' : 'Name'}</Label>
              <Input value={newKeyName} onChange={(e) => setNewKeyName(e.target.value)} placeholder="billing-worker" />
            </div>
            <div className="space-y-1.5">
              <Label>{isRu ? 'Права доступа' : 'Permissions'}</Label>
              <div className="flex flex-wrap gap-2">
                {API_KEY_PERMISSIONS.map((perm) => (
                  <label key={perm} className="flex cursor-pointer items-center gap-1.5 rounded border px-2 py-1 text-xs">
                    <Checkbox
                      checked={newKeyPerms.includes(perm)}
                      onCheckedChange={() =>
                        setNewKeyPerms((prev) =>
                          prev.includes(perm) ? prev.filter((x) => x !== perm) : [...prev, perm],
                        )
                      }
                    />
                    <span>{perm}</span>
                  </label>
                ))}
              </div>
            </div>
            <div className="space-y-1.5">
              <Label>{isRu ? 'Дата истечения (опционально)' : 'Expires At (optional)'}</Label>
              <Input type="date" value={newKeyExpiry} onChange={(e) => setNewKeyExpiry(e.target.value)} />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setNewKeyModalOpen(false)}>
              {isRu ? 'Отмена' : 'Cancel'}
            </Button>
            <Button
              onClick={() => createApiKeyMut.mutate()}
              loading={createApiKeyMut.isPending}
              disabled={!newKeyName.trim()}
            >
              {isRu ? 'Создать' : 'Create'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* One-Time API Key Reveal */}
      <Dialog open={Boolean(createdKeyToken)} onOpenChange={(o) => !o && setCreatedKeyToken(null)}>
        <DialogContent size="md">
          <DialogHeader>
            <DialogTitle>{isRu ? 'API-ключ создан' : 'API Key Created'}</DialogTitle>
            <DialogDescription>
              {isRu
                ? 'Скопируйте токен сейчас — повторно он показан не будет.'
                : 'Copy this token now. It cannot be retrieved again.'}
            </DialogDescription>
          </DialogHeader>
          <div className="flex gap-2">
            <Input readOnly value={createdKeyToken ?? ''} className="font-mono text-xs" />
            <Button
              variant="outline"
              onClick={() => {
                if (!createdKeyToken) return;
                copyToken(createdKeyToken).catch(() => {});
              }}
            >
              {copiedToken ? <Check className="h-4 w-4 text-emerald-400" /> : <Copy className="h-4 w-4" />}
            </Button>
          </div>
          <DialogFooter>
            <Button onClick={() => setCreatedKeyToken(null)}>{isRu ? 'Готово' : 'Done'}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

    </>
  );
}
