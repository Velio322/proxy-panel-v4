import { useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Ban,
  Check,
  Copy,
  Edit,
  KeyRound,
  Plus,
  Search,
  Shield,
  ShieldCheck,
  Sparkles,
  Trash2,
  Users,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
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
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Skeleton } from '@/components/ui/skeleton';
import { PageHero, RackEmptyState, SplitBar } from '@/components/ui/PagePrimitives';
import { getApiErrorMessage, usersApi } from '@/lib/api';
import { formatDateTime, randomPassword } from '@/lib/core-utils';
import { useAuthStore, useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { Language, User, UserRole } from '@/types';

const ROLE_RANK: Record<UserRole, number> = {
  SUPER_ADMIN: 4,
  ADMIN: 3,
  RESELLER: 2,
  OPERATOR: 1,
};

function roleBadgeVariant(role: UserRole) {
  switch (role) {
    case 'SUPER_ADMIN':
      return 'success' as const;
    case 'ADMIN':
      return 'default' as const;
    case 'RESELLER':
      return 'warning' as const;
    default:
      return 'secondary' as const;
  }
}

export function UsersPage() {
  const qc = useQueryClient();
  const currentUser = useAuthStore((s) => s.user);
  const timezone = useUiStore((s) => s.timezone);
  const uiLang = useUiStore((s) => s.language);
  const isRu = uiLang === 'ru';

  const [search, setSearch] = useState('');
  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<User | null>(null);
  const [revealedPassword, setRevealedPassword] = useState<{ username: string; password: string } | null>(
    null,
  );
  const [copiedPass, setCopiedPass] = useState(false);

  const [username, setUsername] = useState('');
  const [email, setEmail] = useState('');
  const [role, setRole] = useState<UserRole>('OPERATOR');
  const [language, setLanguage] = useState<Language>('en');
  const [password, setPassword] = useState('');

  const { data: usersRes, isLoading } = useQuery({
    queryKey: ['users', search],
    queryFn: () => usersApi.list({ search: search.trim() || undefined, limit: 100 }),
  });

  const users = usersRes?.data ?? [];

  const stats = useMemo(() => {
    let active = 0;
    let totp = 0;
    let superAdmins = 0;
    for (const u of users) {
      if (!u.banned) active += 1;
      if (u.totpEnabled) totp += 1;
      if (u.role === 'SUPER_ADMIN' || u.role === 'ADMIN') superAdmins += 1;
    }
    return {
      total: users.length,
      active,
      banned: users.length - active,
      totp,
      superAdmins,
    };
  }, [users]);

  const canManage = (target: User): boolean => {
    if (!currentUser) return false;
    if (currentUser.id === target.id) return false;
    return ROLE_RANK[currentUser.role] > ROLE_RANK[target.role];
  };

  const openCreate = () => {
    setEditing(null);
    setUsername('');
    setEmail('');
    setRole('OPERATOR');
    setLanguage(uiLang);
    setPassword(randomPassword(16));
    setModalOpen(true);
  };

  const openEdit = (u: User) => {
    setEditing(u);
    setUsername(u.username);
    setEmail(u.email ?? '');
    setRole(u.role);
    setLanguage(u.language === 'ru' ? 'ru' : 'en');
    setPassword('');
    setModalOpen(true);
  };

  const saveMut = useMutation({
    mutationFn: async () => {
      if (editing) {
        return usersApi.update(editing.id, {
          email: email.trim() || null,
          role,
          language,
          ...(password.trim() ? { password: password.trim() } : {}),
        });
      }
      return usersApi.create({
        username: username.trim(),
        email: email.trim() || undefined,
        role,
        language,
        password: password.trim() || randomPassword(16),
      });
    },
    onSuccess: (res) => {
      qc.invalidateQueries({ queryKey: ['users'] });
      setModalOpen(false);
      toast({
        title: editing
          ? isRu
            ? 'Пользователь обновлён'
            : 'User updated'
          : isRu
            ? 'Пользователь создан'
            : 'User created',
        variant: 'success',
      });
      const genPass = (res as User & { password?: string }).password || (!editing ? password.trim() : '');
      if (!editing && genPass) {
        setRevealedPassword({ username: res.username, password: genPass });
      }
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка сохранения' : 'Save failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const resetPasswordMut = useMutation({
    mutationFn: async (u: User) => {
      const nextPass = randomPassword(16);
      const res = await usersApi.resetPassword(u.id, nextPass);
      return { username: u.username, password: res.password || nextPass };
    },
    onSuccess: (res) => setRevealedPassword(res),
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка сброса пароля' : 'Reset failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const toggleBanMut = useMutation({
    mutationFn: (id: string) => usersApi.toggleBan(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['users'] }),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => usersApi.delete(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['users'] });
      toast({ title: isRu ? 'Пользователь удалён' : 'User deleted', variant: 'success' });
    },
  });

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Администраторы панели' : 'Panel Users'}
        subtitle={
          isRu
            ? 'Ролевое управление доступом (RBAC), двухфакторная защита TOTP и генерация CSPRNG-паролей'
            : 'Role-based access control (RBAC) with CSPRNG password generation'
        }
        icon={<Users className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs tabular-nums">
            {stats.active}/{stats.total}
          </Badge>
        }
        actions={
          <Button size="sm" onClick={openCreate}>
            <Plus className="h-4 w-4" />
            <span>{isRu ? 'Добавить пользователя' : 'Create User'}</span>
          </Button>
        }
      />

      {/* 3-Card KPI Ribbon */}
      <div className="grid gap-3 sm:grid-cols-3">
        <Card className="p-3 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Аккаунты панели' : 'Panel Accounts'}</span>
            <Users className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {stats.active} / {stats.total}
          </div>
          <SplitBar
            segments={[
              { pct: stats.total > 0 ? (stats.active / stats.total) * 100 : 100, className: 'bg-emerald-400' },
              { pct: stats.total > 0 ? (stats.banned / stats.total) * 100 : 0, className: 'bg-red-500' },
            ]}
          />
        </Card>

        <Card className="p-3 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Защита 2FA (TOTP)' : '2FA Protected'}</span>
            <Shield className="h-4 w-4 text-cyan-400" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {stats.totp} / {stats.total}
          </div>
          <SplitBar
            segments={[
              { pct: stats.total > 0 ? (stats.totp / stats.total) * 100 : 0, className: 'bg-cyan-400' },
            ]}
          />
        </Card>

        <Card className="p-3 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Администраторы' : 'Privileged Admins'}</span>
            <ShieldCheck className="h-4 w-4 text-primary" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">{stats.superAdmins}</div>
          <div className="text-[11px] text-muted-foreground">SUPER_ADMIN / ADMIN</div>
        </Card>
      </div>

      <Card>
        <CardHeader className="py-2.5">
          <div className="relative max-w-xs">
            <Search className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder={isRu ? 'Поиск пользователей...' : 'Search users...'}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="pl-9"
            />
          </div>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="space-y-2 py-2">
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
            </div>
          ) : users.length === 0 ? (
            <RackEmptyState
              icon={Users}
              title={isRu ? 'Пользователи не найдены' : 'No users found'}
            />
          ) : (
            <div className="overflow-x-auto">
              <table className="data-table w-full text-sm">
                <thead className="bg-muted/30">
                  <tr className="border-b text-left text-xs text-muted-foreground">
                    <th className="py-1.5 px-2">{isRu ? 'Логин' : 'Username'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Роль' : 'Role'}</th>
                    <th className="py-1.5 px-2">2FA</th>
                    <th className="py-1.5 px-2">{isRu ? 'Статус' : 'Status'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Последний вход' : 'Last Login'}</th>
                    <th className="py-1.5 px-2 text-right">{isRu ? 'Действия' : 'Actions'}</th>
                  </tr>
                </thead>
                <tbody>
                  {users.map((u) => {
                    const manageable = canManage(u);
                    return (
                      <tr key={u.id} className="border-b last:border-b-0 hover:bg-accent/30">
                        <td className="py-1.5 px-2">
                          <div className="font-semibold">{u.username}</div>
                          <div className="text-xs text-muted-foreground">{u.email || '—'}</div>
                        </td>
                        <td className="py-1.5 px-2">
                          <Badge variant={roleBadgeVariant(u.role)} className="font-mono text-[10px]">
                            {u.role}
                          </Badge>
                        </td>
                        <td className="py-1.5 px-2">
                          <Badge variant={u.totpEnabled ? 'success' : 'secondary'}>
                            {u.totpEnabled ? (isRu ? 'Включена' : 'Enabled') : isRu ? 'Выкл' : 'Off'}
                          </Badge>
                        </td>
                        <td className="py-1.5 px-2">
                          <Badge variant={u.banned ? 'destructive' : 'success'}>
                            {u.banned ? (isRu ? 'Заблокирован' : 'Banned') : isRu ? 'Активен' : 'Active'}
                          </Badge>
                        </td>
                        <td className="py-1.5 px-2 font-mono text-xs text-muted-foreground">
                          {u.lastLoginAt
                            ? formatDateTime(u.lastLoginAt, timezone)
                            : isRu
                              ? 'Никогда'
                              : 'Never'}
                        </td>
                        <td className="py-1.5 px-2 text-right">
                          {manageable ? (
                            <div className="inline-flex gap-1">
                              <Button
                                size="icon"
                                variant="ghost"
                                title={isRu ? 'Сбросить пароль' : 'Reset password'}
                                onClick={() => resetPasswordMut.mutate(u)}
                              >
                                <KeyRound className="h-4 w-4" />
                              </Button>
                              <Button size="icon" variant="ghost" onClick={() => openEdit(u)}>
                                <Edit className="h-4 w-4" />
                              </Button>
                              <Button size="icon" variant="ghost" onClick={() => toggleBanMut.mutate(u.id)}>
                                {u.banned ? (
                                  <ShieldCheck className="h-4 w-4 text-emerald-400" />
                                ) : (
                                  <Ban className="h-4 w-4 text-amber-400" />
                                )}
                              </Button>
                              <Button size="icon" variant="ghost" onClick={() => deleteMut.mutate(u.id)}>
                                <Trash2 className="h-4 w-4 text-red-400" />
                              </Button>
                            </div>
                          ) : (
                            <span className="text-xs text-muted-foreground">
                              {isRu ? 'Защищён' : 'Protected'}
                            </span>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>

      <Dialog open={modalOpen} onOpenChange={setModalOpen}>
        <DialogContent size="md">
          <DialogHeader>
            <DialogTitle>
              {editing
                ? isRu
                  ? `Редактировать: ${editing.username}`
                  : `Edit User: ${editing.username}`
                : isRu
                  ? 'Создать пользователя'
                  : 'Create User'}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            {!editing && (
              <div className="space-y-1.5">
                <Label>{isRu ? 'Логин' : 'Username'}</Label>
                <Input value={username} onChange={(e) => setUsername(e.target.value)} />
              </div>
            )}
            <div className="space-y-1.5">
              <Label>Email</Label>
              <Input type="email" value={email} onChange={(e) => setEmail(e.target.value)} />
            </div>
            <div className="grid gap-2 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Роль' : 'Role'}</Label>
                <Select value={role} onValueChange={(v) => setRole(v as UserRole)}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="OPERATOR">OPERATOR</SelectItem>
                    <SelectItem value="RESELLER">RESELLER</SelectItem>
                    <SelectItem value="ADMIN">ADMIN</SelectItem>
                    {currentUser?.role === 'SUPER_ADMIN' && (
                      <SelectItem value="SUPER_ADMIN">SUPER_ADMIN</SelectItem>
                    )}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Язык' : 'Language'}</Label>
                <Select value={language} onValueChange={(v) => setLanguage(v as Language)}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="en">English</SelectItem>
                    <SelectItem value="ru">Русский</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
            <div className="space-y-1.5">
              <div className="flex items-center justify-between">
                <Label>{editing ? (isRu ? 'Новый пароль (опционально)' : 'New Password (optional)') : isRu ? 'Пароль' : 'Password'}</Label>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="h-6 px-2 text-xs"
                  onClick={() => setPassword(randomPassword(16))}
                >
                  <Sparkles className="h-3 w-3" /> {isRu ? 'Сгенерировать' : 'Generate'}
                </Button>
              </div>
              <Input
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="font-mono text-xs"
              />
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
              disabled={!editing && !username.trim()}
            >
              {isRu ? 'Сохранить' : 'Save User'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={Boolean(revealedPassword)} onOpenChange={(o) => !o && setRevealedPassword(null)}>
        <DialogContent size="md">
          <DialogHeader>
            <DialogTitle>
              {isRu
                ? `Одноразовый пароль: ${revealedPassword?.username}`
                : `One-Time Password: ${revealedPassword?.username}`}
            </DialogTitle>
            <DialogDescription>
              {isRu
                ? 'Скопируйте пароль сейчас — повторно он показан не будет.'
                : 'Copy this password now. It will not be displayed again.'}
            </DialogDescription>
          </DialogHeader>
          <div className="flex gap-2">
            <Input readOnly value={revealedPassword?.password ?? ''} className="font-mono text-xs" />
            <Button
              type="button"
              variant="outline"
              onClick={async () => {
                if (!revealedPassword) return;
                await navigator.clipboard.writeText(revealedPassword.password);
                setCopiedPass(true);
                window.setTimeout(() => setCopiedPass(false), 1500);
              }}
            >
              {copiedPass ? <Check className="h-4 w-4 text-emerald-400" /> : <Copy className="h-4 w-4" />}
            </Button>
          </div>
          <DialogFooter>
            <Button onClick={() => setRevealedPassword(null)}>{isRu ? 'Готово' : 'Done'}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
