import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { getApiErrorMessage, settingsApi } from '@/lib/api';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';

export function useSettingsForm(isSuperAdmin: boolean, isRu: boolean) {
  const qc = useQueryClient();
  const setTimezone = useUiStore((s) => s.setTimezone);
  const setCompanyName = useUiStore((s) => s.setCompanyName);

  const { data: rawSettings } = useQuery({
    queryKey: ['settings'],
    queryFn: () => settingsApi.get(),
    enabled: isSuperAdmin,
  });

  const [formSettings, setFormSettings] = useState<Record<string, string>>({});
  const [dirtyKeys, setDirtyKeys] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (rawSettings) {
      setFormSettings(rawSettings);
      setDirtyKeys(new Set());
    }
  }, [rawSettings]);

  const setSetting = (key: string, val: string) => {
    setFormSettings((prev) => ({ ...prev, [key]: val }));
    setDirtyKeys((prev) => new Set(prev).add(key));
  };

  const saveSettingsMut = useMutation({
    mutationFn: async () => {
      const payload: Record<string, string> = {};
      for (const k of dirtyKeys) {
        const v = formSettings[k];
        if (v === '********') continue;
        payload[k] = v ?? '';
      }
      if (Object.keys(payload).length === 0) return false;
      await settingsApi.update(payload);
      if (payload.timezone) setTimezone(payload.timezone);
      if (payload.company_name) setCompanyName(payload.company_name);
      return true;
    },
    onSuccess: (saved) => {
      if (!saved) return;
      qc.invalidateQueries({ queryKey: ['settings'] });
      setDirtyKeys(new Set());
      toast({ title: isRu ? 'Настройки сохранены' : 'Settings saved', variant: 'success' });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка сохранения' : 'Save failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  return { formSettings, setSetting, saveSettingsMut };
}
