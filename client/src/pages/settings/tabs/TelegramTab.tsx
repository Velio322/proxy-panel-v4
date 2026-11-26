import { useTranslation } from '@/lib/i18n';
import { useAuthStore } from '@/lib/store';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { TabsContent } from '@/components/ui/tabs';
import { useSettingsForm } from '../use-settings-form';

export function SettingsTelegramTab() {
  const { language } = useTranslation();
  const isRu = language === 'ru';
  const isSuperAdmin = useAuthStore((s) => s.user?.role === 'SUPER_ADMIN');
  const { formSettings, setSetting, saveSettingsMut } = useSettingsForm(isSuperAdmin, isRu);

  return (
        <TabsContent value="telegram">
          <Card>
            <CardHeader>
              <CardTitle>{isRu ? 'Telegram-бот и уведомления' : 'Telegram Bot & Notifications'}</CardTitle>
              <CardDescription>
                {isRu
                  ? 'Скрытые токены (********) сохраняются без изменений, если поле не перезаписано.'
                  : 'Masked tokens (********) are preserved unless replaced.'}
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid gap-2 sm:grid-cols-2">
                <div className="space-y-1.5">
                  <Label>Bot Token</Label>
                  <Input
                    type="password"
                    value={formSettings.telegram_bot_token ?? ''}
                    onChange={(e) => setSetting('telegram_bot_token', e.target.value)}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>{isRu ? 'Username бота (для Telegram OAuth)' : 'Bot Username (for Telegram OAuth Widget)'}</Label>
                  <Input
                    placeholder="my_proxy_bot"
                    value={formSettings.telegram_bot_username ?? ''}
                    onChange={(e) => setSetting('telegram_bot_username', e.target.value)}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>{isRu ? 'Chat ID администраторов (через запятую)' : 'Admin Chat IDs (comma-separated)'}</Label>
                  <Input
                    value={formSettings.telegram_admin_ids ?? ''}
                    onChange={(e) => setSetting('telegram_admin_ids', e.target.value)}
                  />
                </div>
                <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                  <Label>{isRu ? 'Уведомления включены' : 'Notifications Enabled'}</Label>
                  <Switch
                    checked={formSettings.telegram_notifications === 'true'}
                    onCheckedChange={(v) => setSetting('telegram_notifications', String(v))}
                  />
                </div>
              </div>
              {isSuperAdmin && (
                <div className="flex justify-end">
                  <Button onClick={() => saveSettingsMut.mutate()} loading={saveSettingsMut.isPending}>
                    {isRu ? 'Сохранить настройки Telegram' : 'Save Telegram Settings'}
                  </Button>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>
  );
}
