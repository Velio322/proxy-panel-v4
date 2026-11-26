import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { startRegistration, browserSupportsWebAuthn } from '@simplewebauthn/browser';
import { QRCodeSVG } from 'qrcode.react';
import { KeyRound, Plus, Shield, Trash2 } from 'lucide-react';
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
import { Label } from '@/components/ui/label';
import { RackEmptyState } from '@/components/ui/PagePrimitives';
import { TabsContent } from '@/components/ui/tabs';
import { authApi, getApiErrorMessage } from '@/lib/api';
import { formatDateTime } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore, useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';

export function SettingsSecurityTab() {
  const qc = useQueryClient();
  const { language } = useTranslation();
  const isRu = language === 'ru';
  const loadUser = useAuthStore((s) => s.loadUser);
  const user = useAuthStore((s) => s.user);
  const timezone = useUiStore((s) => s.timezone);

  // --- Security: 2FA, Passkeys, Password
  const [setup2faData, setSetup2faData] = useState<{ secret: string; otpauthUrl: string } | null>(null);
  const [totpVerifyCode, setTotpVerifyCode] = useState('');
  const [disable2faOpen, setDisable2faOpen] = useState(false);
  const [disableCode, setDisableCode] = useState('');

  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');

  const [passkeyName, setPasskeyName] = useState('');

  const { data: passkeys = [] } = useQuery({
    queryKey: ['passkeys'],
    queryFn: () => authApi.listPasskeys(),
  });

  const start2faMut = useMutation({
    mutationFn: () => authApi.setup2fa(),
    onSuccess: (res) => setSetup2faData({ secret: res.secret, otpauthUrl: res.otpauthUrl }),
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка настройки 2FA' : '2FA setup failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const enable2faMut = useMutation({
    mutationFn: () => authApi.enable2fa(totpVerifyCode.trim()),
    onSuccess: async () => {
      setSetup2faData(null);
      setTotpVerifyCode('');
      await loadUser();
      toast({ title: isRu ? '2FA включена' : '2FA enabled', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Неверный код' : 'Invalid code',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const disable2faMut = useMutation({
    mutationFn: () => authApi.disable2fa({ code: disableCode.trim() }),
    onSuccess: async () => {
      setDisable2faOpen(false);
      setDisableCode('');
      await loadUser();
      toast({ title: isRu ? '2FA отключена' : '2FA disabled', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка отключения' : 'Disable failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const changePassMut = useMutation({
    mutationFn: () => authApi.changePassword({ currentPassword, newPassword }),
    onSuccess: () => {
      setCurrentPassword('');
      setNewPassword('');
      toast({ title: isRu ? 'Пароль обновлён' : 'Password updated', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка смены пароля' : 'Password change failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const registerPasskeyMut = useMutation({
    mutationFn: async () => {
      const { options, challengeId } = await authApi.startPasskeyRegistration();
      const credential = await startRegistration({ optionsJSON: options });
      return authApi.finishPasskeyRegistration({
        challengeId,
        name: passkeyName.trim() || 'Security Key',
        credential,
      });
    },
    onSuccess: () => {
      setPasskeyName('');
      qc.invalidateQueries({ queryKey: ['passkeys'] });
      toast({ title: isRu ? 'Passkey зарегистрирован' : 'Passkey registered', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка Passkey' : 'Passkey error',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const deletePasskeyMut = useMutation({
    mutationFn: (id: string) => authApi.deletePasskey(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['passkeys'] }),
  });


  return (
    <>
        <TabsContent value="security" className="space-y-4">
          <div className="grid gap-3 lg:grid-cols-2">
            {/* 2FA TOTP */}
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Shield className="h-4 w-4 text-primary" />
                  <span>
                    {isRu ? 'Двухфакторная аутентификация (TOTP)' : 'Two-Factor Authentication (TOTP)'}
                  </span>
                </CardTitle>
                <CardDescription>
                  {isRu ? 'Статус: ' : 'Status: '}
                  <Badge variant={user?.totpEnabled ? 'success' : 'secondary'}>
                    {user?.totpEnabled
                      ? isRu
                        ? 'Включена'
                        : 'Enabled'
                      : isRu
                        ? 'Отключена'
                        : 'Disabled'}
                  </Badge>
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                {user?.totpEnabled ? (
                  <Button variant="destructive" size="sm" onClick={() => setDisable2faOpen(true)}>
                    {isRu ? 'Отключить 2FA' : 'Disable 2FA'}
                  </Button>
                ) : setup2faData ? (
                  <div className="space-y-3">
                    <div className="flex justify-center rounded-lg bg-white p-3 w-fit">
                      <QRCodeSVG value={setup2faData.otpauthUrl} size={140} />
                    </div>
                    <div className="text-xs font-mono break-all text-muted-foreground">
                      Secret: {setup2faData.secret}
                    </div>
                    <div className="flex gap-2">
                      <Input
                        placeholder={isRu ? '6-значный код' : '6-digit code'}
                        maxLength={6}
                        value={totpVerifyCode}
                        onChange={(e) => setTotpVerifyCode(e.target.value.replace(/\D/g, ''))}
                      />
                      <Button onClick={() => enable2faMut.mutate()} loading={enable2faMut.isPending}>
                        {isRu ? 'Подтвердить и включить' : 'Verify & Enable'}
                      </Button>
                    </div>
                  </div>
                ) : (
                  <Button size="sm" onClick={() => start2faMut.mutate()} loading={start2faMut.isPending}>
                    {isRu ? 'Настроить 2FA' : 'Setup 2FA'}
                  </Button>
                )}
              </CardContent>
            </Card>

            {/* Change Password */}
            <Card>
              <CardHeader>
                <CardTitle>{isRu ? 'Смена пароля' : 'Change Password'}</CardTitle>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="space-y-1.5">
                  <Label>{isRu ? 'Текущий пароль' : 'Current Password'}</Label>
                  <Input
                    type="password"
                    value={currentPassword}
                    onChange={(e) => setCurrentPassword(e.target.value)}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>{isRu ? 'Новый пароль' : 'New Password'}</Label>
                  <Input
                    type="password"
                    value={newPassword}
                    onChange={(e) => setNewPassword(e.target.value)}
                  />
                </div>
                <Button
                  size="sm"
                  onClick={() => changePassMut.mutate()}
                  loading={changePassMut.isPending}
                  disabled={!currentPassword || newPassword.length < 8}
                >
                  {isRu ? 'Обновить пароль' : 'Update Password'}
                </Button>
              </CardContent>
            </Card>
          </div>

          {/* Passkeys / WebAuthn */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <KeyRound className="h-4 w-4 text-primary" />
                <span>Passkeys (WebAuthn)</span>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="flex flex-wrap gap-2">
                <Input
                  placeholder={
                    isRu ? 'Название ключа (например, YubiKey / MacBook)' : 'Passkey device label (e.g. YubiKey / MacBook)'
                  }
                  value={passkeyName}
                  onChange={(e) => setPasskeyName(e.target.value)}
                  className="max-w-xs"
                />
                <Button
                  size="sm"
                  disabled={!browserSupportsWebAuthn()}
                  onClick={() => registerPasskeyMut.mutate()}
                  loading={registerPasskeyMut.isPending}
                >
                  <Plus className="h-4 w-4" />
                  <span>{isRu ? 'Зарегистрировать Passkey' : 'Register Passkey'}</span>
                </Button>
              </div>
              {passkeys.length === 0 ? (
                <RackEmptyState
                  icon={KeyRound}
                  title={isRu ? 'Аппаратные ключи Passkey не добавлены' : 'No passkeys registered'}
                  description={
                    isRu
                      ? 'Зарегистрируйте WebAuthn/Passkey ключ для входа без пароля.'
                      : 'Register a WebAuthn passkey for passwordless authentication.'
                  }
                />
              ) : (
                <div className="space-y-2">
                  {passkeys.map((pk) => (
                    <div
                      key={pk.id}
                      className="rack-inset flex items-center justify-between rounded-xl px-2.5 py-1.5 text-xs"
                    >
                      <div>
                        <div className="font-semibold">{pk.name || pk.id}</div>
                        <div className="text-muted-foreground">
                          {isRu ? 'Добавлен: ' : 'Added '}
                          {formatDateTime(pk.createdAt, timezone)}
                        </div>
                      </div>
                      <Button size="icon" variant="ghost" onClick={() => deletePasskeyMut.mutate(pk.id)}>
                        <Trash2 className="h-4 w-4 text-red-400" />
                      </Button>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>


      {/* Disable 2FA Modal */}
      <Dialog open={disable2faOpen} onOpenChange={setDisable2faOpen}>
        <DialogContent size="sm">
          <DialogHeader>
            <DialogTitle>{isRu ? 'Отключить 2FA' : 'Disable 2FA'}</DialogTitle>
            <DialogDescription>
              {isRu
                ? 'Введите 6-значный TOTP-код для отключения двухфакторной защиты.'
                : 'Enter a valid 6-digit TOTP code to disable 2FA.'}
            </DialogDescription>
          </DialogHeader>
          <Input
            maxLength={6}
            placeholder="000000"
            value={disableCode}
            onChange={(e) => setDisableCode(e.target.value.replace(/\D/g, ''))}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setDisable2faOpen(false)}>
              {isRu ? 'Отмена' : 'Cancel'}
            </Button>
            <Button variant="destructive" onClick={() => disable2faMut.mutate()} loading={disable2faMut.isPending}>
              {isRu ? 'Отключить' : 'Disable'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
