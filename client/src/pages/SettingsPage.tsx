import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { startRegistration, browserSupportsWebAuthn } from '@simplewebauthn/browser';
import { QRCodeSVG } from 'qrcode.react';
import {
  Check,
  Copy,
  Crop,
  Download,
  ExternalLink,
  HardDrive,
  ImagePlus,
  Key,
  KeyRound,
  Palette,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  Settings,
  Shield,
  Sparkles,
  Trash2,
  Upload,
  Webhook,
} from 'lucide-react';
import { BrandMark, DEFAULT_PANEL_NAME } from '@/components/branding/BrandLogo';
import { LogoCropDialog } from '@/components/branding/LogoCropDialog';
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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Checkbox } from '@/components/ui/checkbox';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';
import { PageHero, RackEmptyState } from '@/components/ui/PagePrimitives';
import { LANGUAGES } from '@/i18n/translations';
import {
  authApi,
  backupApi,
  brandingApi,
  coresApi,
  getApiErrorMessage,
  nodesApi,
  settingsApi,
  usersApi,
  webhooksApi,
} from '@/lib/api';
import { formatDateTime, randomHex } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore, usePublicConfigStore, useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { Language, WebhookItem } from '@/types';
import { useCopyFeedback } from '@/hooks/use-copy-feedback';

const WEBHOOK_SCOPES = ['user', 'node', 'service'] as const;
const WEBHOOK_EVENTS = [
  'user.created',
  'user.expired',
  'user.traffic_threshold',
  'user.first_connected',
  'node.offline',
  'node.restored',
  'service.backup_completed',
  'service.backup_failed',
] as const;

const API_KEY_PERMISSIONS = ['read', 'write', 'clients:manage', 'nodes:manage'] as const;

export function SettingsPage() {
  const { t, language, setLanguage } = useTranslation();
  const qc = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();
  const initialTab = searchParams.get('tab') || 'general';
  const [activeTab, setActiveTab] = useState(initialTab);

  const user = useAuthStore((s) => s.user);
  const loadUser = useAuthStore((s) => s.loadUser);
  const timezone = useUiStore((s) => s.timezone);
  const setTimezone = useUiStore((s) => s.setTimezone);
  const setCompanyName = useUiStore((s) => s.setCompanyName);
  const healthVersion = usePublicConfigStore((s) => s.healthVersion);
  const branding = usePublicConfigStore((s) => s.branding);
  const setBranding = usePublicConfigStore((s) => s.setBranding);

  const isSuperAdmin = user?.role === 'SUPER_ADMIN';
  const canEditBranding = user?.role === 'SUPER_ADMIN' || user?.role === 'ADMIN';
  const isRu = language === 'ru';

  // --- Appearance & Branding
  const [brandName, setBrandName] = useState(branding.panelName || DEFAULT_PANEL_NAME);
  const [logoMode, setLogoMode] = useState<'upload' | 'svg'>('upload');
  const [pendingLogoDataUrl, setPendingLogoDataUrl] = useState<string | null>(null);
  const [rawCropSourceUrl, setRawCropSourceUrl] = useState<string | null>(null);
  const [cropDialogOpen, setCropDialogOpen] = useState(false);
  const [pendingSvgCode, setPendingSvgCode] = useState('');
  const [clearLogoFlag, setClearLogoFlag] = useState(false);
  const [logoError, setLogoError] = useState<string | null>(null);

  useEffect(() => {
    setBrandName(branding.panelName || DEFAULT_PANEL_NAME);
  }, [branding.panelName]);

  const trimmedSvgDraft = pendingSvgCode.trim();
  const previewPanelName = brandName.trim() || DEFAULT_PANEL_NAME;
  const previewLogoUrl = clearLogoFlag
    ? null
    : logoMode === 'svg' && trimmedSvgDraft.toLowerCase().includes('<svg')
      ? `data:image/svg+xml;charset=utf-8,${encodeURIComponent(trimmedSvgDraft)}`
      : pendingLogoDataUrl ?? (branding.hasCustomLogo ? branding.logoUrl ?? null : null);

  const handleLogoFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    setLogoError(null);

    const lowerName = file.name.toLowerCase();
    const isSvgFile = lowerName.endsWith('.svg') || file.type === 'image/svg+xml';
    const isRasterFile =
      lowerName.endsWith('.png') ||
      lowerName.endsWith('.webp') ||
      lowerName.endsWith('.jpg') ||
      lowerName.endsWith('.jpeg') ||
      file.type === 'image/png' ||
      file.type === 'image/webp' ||
      file.type === 'image/jpeg';

    if (!isSvgFile && !isRasterFile) {
      setLogoError(
        isRu
          ? 'Допустимы только файлы .png, .jpg, .jpeg, .webp или .svg'
          : 'Only .png, .jpg, .jpeg, .webp, or .svg files are allowed',
      );
      return;
    }

    const maxBytes = isSvgFile ? 2 * 1024 * 1024 : 8 * 1024 * 1024;
    if (file.size > maxBytes) {
      setLogoError(
        isRu
          ? `Размер файла превышает ${isSvgFile ? '2 МБ' : '8 МБ'}`
          : `File size exceeds ${isSvgFile ? '2 MB' : '8 MB'} limit`,
      );
      return;
    }

    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result !== 'string') return;
      setLogoMode('upload');
      setPendingSvgCode('');
      setClearLogoFlag(false);
      setRawCropSourceUrl(reader.result);

      if (isSvgFile) {
        // Direct apply for SVG without mandatory raster cropping, with optional crop button
        setPendingLogoDataUrl(reader.result);
      } else {
        // Open Telegram-style interactive crop modal for raster images
        setCropDialogOpen(true);
      }
    };
    reader.onerror = () => {
      setLogoError(isRu ? 'Не удалось прочитать файл' : 'Failed to read file');
    };
    reader.readAsDataURL(file);
  };

  const saveBrandingMut = useMutation({
    mutationFn: (opts?: { forceClearLogo?: boolean; logoDataUrlOverride?: string }) => {
      const clearLogo = opts?.forceClearLogo ?? clearLogoFlag;
      const effectiveDataUrl =
        opts?.logoDataUrlOverride !== undefined
          ? opts.logoDataUrlOverride
          : !clearLogo && logoMode === 'upload'
            ? pendingLogoDataUrl ?? undefined
            : undefined;
      return brandingApi.update({
        panelName: previewPanelName,
        logoDataUrl: effectiveDataUrl,
        svgCode:
          !clearLogo && !opts?.logoDataUrlOverride && logoMode === 'svg' && trimmedSvgDraft
            ? trimmedSvgDraft
            : undefined,
        clearLogo,
      });
    },
    onSuccess: (updated) => {
      setBranding(updated);
      setBrandName(updated.panelName || DEFAULT_PANEL_NAME);
      setPendingLogoDataUrl(null);
      setPendingSvgCode('');
      setClearLogoFlag(false);
      setLogoError(null);
      qc.invalidateQueries({ queryKey: ['settings'] });
      toast({
        title: isRu ? 'Брендинг панели обновлён' : 'Panel branding updated',
        variant: 'success',
      });
    },
    onError: (err) => {
      const msg = getApiErrorMessage(err);
      setLogoError(msg);
      toast({
        title: isRu ? 'Ошибка сохранения брендинга' : 'Failed to update branding',
        description: msg,
        variant: 'destructive',
      });
    },
  });

  const handleApplyCroppedLogo = (croppedDataUrl: string) => {
    setLogoMode('upload');
    setPendingLogoDataUrl(croppedDataUrl);
    setPendingSvgCode('');
    setClearLogoFlag(false);
    setLogoError(null);
    setCropDialogOpen(false);
    saveBrandingMut.mutate({ logoDataUrlOverride: croppedDataUrl });
  };

  const resetBrandingMut = useMutation({
    mutationFn: () => brandingApi.reset(),
    onSuccess: (resetRes) => {
      setBranding(resetRes);
      setBrandName(resetRes.panelName || DEFAULT_PANEL_NAME);
      setPendingLogoDataUrl(null);
      setRawCropSourceUrl(null);
      setPendingSvgCode('');
      setClearLogoFlag(false);
      setLogoError(null);
      qc.invalidateQueries({ queryKey: ['settings'] });
      toast({
        title: isRu ? 'Стандартное оформление восстановлено' : 'Reset to default Keeper branding',
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка сброса' : 'Reset failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  // --- General & Telegram Settings
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
      if (Object.keys(payload).length === 0) return;
      await settingsApi.update(payload);
      if (payload.timezone) setTimezone(payload.timezone);
      if (payload.company_name) setCompanyName(payload.company_name);
    },
    onSuccess: () => {
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
    enabled: Boolean(effectiveKeyUserId),
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

  // --- Cores & Backup
  const [updatingCoreKey, setUpdatingCoreKey] = useState<string | null>(null);
  const [refreshingCores, setRefreshingCores] = useState(false);

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
  });

  const { data: coreCatalog = [] } = useQuery({
    queryKey: ['cores-versions'],
    queryFn: () => coresApi.versions(),
  });

  const handleRefreshCores = async () => {
    setRefreshingCores(true);
    try {
      const fresh = await coresApi.versions(true);
      qc.setQueryData(['cores-versions'], fresh);
      qc.invalidateQueries({ queryKey: ['nodes'] });
      toast({
        title: isRu ? 'Список релизов обновлён' : 'Core catalog refreshed',
        variant: 'success',
      });
    } catch (err) {
      toast({
        title: isRu ? 'Ошибка проверки обновлений' : 'Failed to check updates',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      });
    } finally {
      setRefreshingCores(false);
    }
  };

  const updateCoreMut = useMutation({
    mutationFn: ({ nodeId, core }: { nodeId: string; core: 'xray' | 'singbox' | 'naive' | 'mieru' | 'telemt' }) =>
      nodesApi.coreUpdate(nodeId, { core }),
    onMutate: (vars) => setUpdatingCoreKey(`${vars.nodeId}:${vars.core}`),
    onSuccess: (res, vars) => {
      qc.invalidateQueries({ queryKey: ['nodes'] });
      qc.invalidateQueries({ queryKey: ['node', vars.nodeId] });
      toast({
        title: isRu
          ? `${vars.core}: обновлено${res.version ? ` до ${res.version}` : ''}`
          : `${vars.core} updated${res.version ? ` to ${res.version}` : ''}`,
        description: res.sha256 ? `SHA-256: ${res.sha256.slice(0, 16)}...` : res.message,
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка обновления ядра' : 'Core update failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
    onSettled: () => setUpdatingCoreKey(null),
  });

  const { data: backupLogs = [] } = useQuery({
    queryKey: ['backup-logs'],
    queryFn: () => backupApi.logs(),
    enabled: isSuperAdmin,
  });

  const triggerBackupMut = useMutation({
    mutationFn: () => backupApi.trigger({ type: 'DATABASE', destination: 'local' }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['backup-logs'] });
      toast({ title: isRu ? 'Бэкап запущен' : 'Backup started', variant: 'success' });
    },
  });

  return (
    <div className="space-y-5">
      <PageHero
        title={t('settings.title')}
        subtitle={
          isRu
            ? 'Параметры панели управления, безопасность, API-токены, вебхуки, Telegram-бот и обновления прокси-ядер'
            : 'Control plane configuration, security, API tokens, webhooks, Telegram integration, and proxy core updates'
        }
        icon={<Settings className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs">
            {healthVersion ? `v${healthVersion.replace(/^v/, '')}` : 'v4.0'}
          </Badge>
        }
        actions={
          <a
            href="/api/docs"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1.5 rounded-lg border border-border/80 bg-background/60 px-3 py-1.5 text-xs font-medium transition-colors hover:border-primary/45 hover:bg-accent"
          >
            <span>{isRu ? 'API Документация (Scalar)' : 'Open API Documentation (Scalar)'}</span>
            <ExternalLink className="h-3.5 w-3.5" />
          </a>
        }
      />

      <Tabs
        value={activeTab}
        onValueChange={(tab) => {
          setActiveTab(tab);
          setSearchParams({ tab });
        }}
      >
        <TabsList className="flex-wrap">
          <TabsTrigger value="general">{t('settings.general')}</TabsTrigger>
          {canEditBranding && <TabsTrigger value="branding">{t('settings.branding')}</TabsTrigger>}
          <TabsTrigger value="security">{t('settings.security')}</TabsTrigger>
          <TabsTrigger value="api-keys">{t('settings.apiKeys')}</TabsTrigger>
          <TabsTrigger value="webhooks">{t('settings.webhooks')}</TabsTrigger>
          <TabsTrigger value="telegram">{t('settings.telegram')}</TabsTrigger>
          <TabsTrigger value="cores">{t('settings.cores')}</TabsTrigger>
          <TabsTrigger value="backup">{t('settings.backup')}</TabsTrigger>
        </TabsList>

        {canEditBranding && (
          <TabsContent value="branding" className="space-y-4">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Palette className="h-4 w-4 text-primary" />
                  <span>{isRu ? 'Внешний вид и брендинг' : 'Appearance & Branding'}</span>
                </CardTitle>
                <CardDescription>
                  {isRu
                    ? 'Настройте отображаемое название и логотип (.svg, .png, .webp до 1 МБ). Изменения сразу применяются к сайдбару, экрану входа и favicon браузера без пересборки.'
                    : 'Customize the panel display name and icon (.svg, .png, .webp up to 1 MB). Changes apply immediately to Sidebar, Login page, title, and browser favicon without rebuilding.'}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-6">
                {/* Real-time 3-surface Preview */}
                <div className="space-y-2">
                  <Label className="text-xs uppercase tracking-wider text-muted-foreground">
                    {isRu ? 'Предпросмотр в реальном времени' : 'Live Preview'}
                  </Label>
                  <div className="grid gap-2 lg:grid-cols-3">
                    {/* 1. Mini Browser Tab */}
                    <div className="rounded-xl border border-border/80 bg-muted/30 p-3 space-y-2">
                      <div className="text-[11px] font-mono uppercase tracking-wider text-muted-foreground">
                        {isRu ? 'Вкладка браузера (Favicon)' : 'Browser Tab (Favicon)'}
                      </div>
                      <div className="flex items-center gap-2 rounded-t-lg border border-b-0 border-border/80 bg-background px-3 py-2 shadow-sm">
                        <BrandMark logoUrl={previewLogoUrl} panelName={previewPanelName} size="xs" />
                        <span className="break-words text-xs font-medium min-w-0">{previewPanelName}</span>
                      </div>
                      <div className="h-2 rounded-b bg-background/70 border border-t-0 border-border/60" />
                    </div>

                    {/* 2. Mini Sidebar Header */}
                    <div className="rounded-xl border border-border/80 bg-muted/30 p-3 space-y-2">
                      <div className="text-[11px] font-mono uppercase tracking-wider text-muted-foreground">
                        {isRu ? 'Шапка сайдбара' : 'Sidebar Header'}
                      </div>
                      <div className="keeper-sidebar keeper-sidebar-brand flex items-center gap-3 rounded-lg border border-border/80 px-3 py-2.5">
                        <BrandMark logoUrl={previewLogoUrl} panelName={previewPanelName} size="md" />
                        <div className="min-w-0">
                          <div className="break-words text-base font-bold leading-tight min-w-0">{previewPanelName}</div>
                          <div className="mt-1 inline-flex items-center gap-1 rounded border border-emerald-500/25 bg-emerald-500/10 px-1.5 py-[1px] font-mono text-[9px] font-semibold uppercase tracking-[0.12em] text-emerald-500 dark:text-emerald-300">
                            <span className="h-1 w-1 rounded-full bg-emerald-400" />
                            <span>CORE · {healthVersion ? `v${healthVersion.replace(/^v/i, '')}` : 'v4.0.0'}</span>
                          </div>
                        </div>
                      </div>
                    </div>

                    {/* 3. Mini Login Card */}
                    <div className="rounded-xl border border-border/80 bg-muted/30 p-3 space-y-2">
                      <div className="text-[11px] font-mono uppercase tracking-wider text-muted-foreground">
                        {isRu ? 'Экран входа (Login)' : 'Login Card'}
                      </div>
                      <div className="login-grid-bg flex flex-col items-center justify-center gap-2 rounded-lg border border-border/80 bg-background/90 py-3 px-4 text-center">
                        <BrandMark logoUrl={previewLogoUrl} panelName={previewPanelName} size="lg" />
                        <div className="break-words max-w-full text-sm font-bold min-w-0">{previewPanelName}</div>
                      </div>
                    </div>
                  </div>
                </div>

                {/* Panel Name */}
                <div className="max-w-md space-y-1.5">
                  <Label htmlFor="branding-panel-name">
                    {isRu ? 'Отображаемое название панели' : 'Display Panel Name'}
                  </Label>
                  <Input
                    id="branding-panel-name"
                    maxLength={64}
                    placeholder={DEFAULT_PANEL_NAME}
                    value={brandName}
                    onChange={(e) => setBrandName(e.target.value)}
                  />
                  <p className="text-[11px] text-muted-foreground">
                    {isRu
                      ? 'До 64 символов. Используется в шапке меню, на экране входа и в заголовке вкладки.'
                      : 'Up to 64 characters. Displayed in Sidebar, Login page, and browser tab title.'}
                  </p>
                </div>

                {/* Custom Icon Input */}
                <div className="space-y-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <Label>{isRu ? 'Иконка / логотип панели' : 'Panel Icon / Logo'}</Label>
                    <div className="inline-flex rounded-lg border border-border/80 bg-muted/40 p-0.5">
                      <button
                        type="button"
                        onClick={() => {
                          setLogoMode('upload');
                          setLogoError(null);
                        }}
                        className={`rounded-md px-2.5 py-1 text-xs font-medium transition-colors ${
                          logoMode === 'upload'
                            ? 'bg-background text-foreground shadow-sm'
                            : 'text-muted-foreground hover:text-foreground'
                        }`}
                      >
                        {isRu ? 'Загрузить файл' : 'Upload File'}
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          setLogoMode('svg');
                          setLogoError(null);
                        }}
                        className={`rounded-md px-2.5 py-1 text-xs font-medium transition-colors ${
                          logoMode === 'svg'
                            ? 'bg-background text-foreground shadow-sm'
                            : 'text-muted-foreground hover:text-foreground'
                        }`}
                      >
                        {isRu ? 'Вставить SVG-код' : 'Paste SVG Code'}
                      </button>
                    </div>
                  </div>

                  {logoMode === 'upload' ? (
                    <div className="space-y-2.5">
                      <label className="flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-border/90 bg-muted/20 px-4 py-6 text-center transition-colors hover:border-primary/50 hover:bg-accent/30">
                        <Upload className="h-5 w-5 text-primary" />
                        <div className="text-xs font-medium">
                          {isRu
                            ? 'Выберите .png, .jpg, .webp (откроется редактор обрезки) или .svg'
                            : 'Choose .png, .jpg, .webp (opens interactive cropper) or .svg'}
                        </div>
                        <div className="text-[11px] text-muted-foreground">
                          {isRu
                            ? 'Растровые картинки автоматически кадрируются в 256×256 px (< 150 КБ)'
                            : 'Raster images are interactively cropped to 256×256 px (< 150 KB)'}
                        </div>
                        {pendingLogoDataUrl && (
                          <div className="inline-flex items-center gap-1.5 rounded-md border border-emerald-500/35 bg-emerald-500/10 px-2 py-0.5 text-[11px] text-emerald-500 dark:text-emerald-300">
                            <ImagePlus className="h-3 w-3" />
                            <span>
                              {isRu ? 'Изображение подготовлено к сохранению' : 'Logo staged for saving'}
                            </span>
                          </div>
                        )}
                        <input
                          type="file"
                          accept=".png,.jpg,.jpeg,.webp,.svg,image/png,image/jpeg,image/webp,image/svg+xml"
                          onChange={handleLogoFileChange}
                          className="sr-only"
                        />
                      </label>

                      {(rawCropSourceUrl || previewLogoUrl) && (
                        <div className="flex items-center justify-end">
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="h-8 gap-1.5 text-xs"
                            onClick={() => {
                              if (!rawCropSourceUrl && previewLogoUrl) {
                                setRawCropSourceUrl(previewLogoUrl);
                              }
                              setCropDialogOpen(true);
                            }}
                          >
                            <Crop className="h-3.5 w-3.5 text-primary" />
                            <span>
                              {isRu ? 'Открыть в редакторе обрезки' : 'Open in Crop Editor'}
                            </span>
                          </Button>
                        </div>
                      )}
                    </div>
                  ) : (
                    <div className="space-y-2">
                      <Textarea
                        rows={5}
                        className="font-mono text-xs"
                        placeholder='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40">...</svg>'
                        value={pendingSvgCode}
                        onChange={(e) => {
                          setPendingSvgCode(e.target.value);
                          setClearLogoFlag(false);
                          setLogoError(null);
                        }}
                      />
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <p className="text-[11px] text-muted-foreground">
                          {isRu
                            ? 'Сервер проверяет SVG и блокирует скрипты, внешние DOCTYPE и обработчики событий (on*=).'
                            : 'SVG markup is sanitized on save: <script>, DOCTYPE/ENTITY, and inline event handlers are rejected.'}
                        </p>
                        {trimmedSvgDraft.toLowerCase().includes('<svg') && (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="h-7 gap-1.5 text-xs"
                            onClick={() => {
                              setRawCropSourceUrl(
                                `data:image/svg+xml;charset=utf-8,${encodeURIComponent(trimmedSvgDraft)}`,
                              );
                              setCropDialogOpen(true);
                            }}
                          >
                            <Crop className="h-3 w-3 text-primary" />
                            <span>{isRu ? 'Открыть в редакторе' : 'Open in Cropper'}</span>
                          </Button>
                        )}
                      </div>
                    </div>
                  )}

                  {logoError && (
                    <div className="rounded-lg border border-red-500/35 bg-red-500/10 px-3 py-2 text-xs text-red-500 dark:text-red-400">
                      {logoError}
                    </div>
                  )}
                </div>

                {/* Branding Actions */}
                <div className="flex flex-wrap items-center justify-between gap-2 border-t pt-4">
                  <div className="flex flex-wrap items-center gap-2">
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={saveBrandingMut.isPending || resetBrandingMut.isPending}
                      onClick={() => resetBrandingMut.mutate()}
                    >
                      <RotateCcw className="h-3.5 w-3.5" />
                      <span>{isRu ? 'Сбросить к стандарту Keeper' : 'Reset to Default'}</span>
                    </Button>

                    {(branding.hasCustomLogo || pendingLogoDataUrl || trimmedSvgDraft) && (
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        disabled={saveBrandingMut.isPending || resetBrandingMut.isPending}
                        onClick={() => {
                          setPendingLogoDataUrl(null);
                          setRawCropSourceUrl(null);
                          setPendingSvgCode('');
                          setLogoError(null);
                          if (branding.hasCustomLogo) {
                            saveBrandingMut.mutate({ forceClearLogo: true });
                          } else {
                            setClearLogoFlag(false);
                          }
                        }}
                      >
                        <Trash2 className="h-3.5 w-3.5 text-red-400" />
                        <span>{isRu ? 'Убрать логотип' : 'Remove Logo'}</span>
                      </Button>
                    )}
                  </div>

                  <Button
                    type="button"
                    loading={saveBrandingMut.isPending}
                    disabled={resetBrandingMut.isPending}
                    onClick={() => saveBrandingMut.mutate({})}
                  >
                    {isRu ? 'Сохранить брендинг' : 'Save Branding'}
                  </Button>
                </div>
              </CardContent>
            </Card>

            <LogoCropDialog
              open={cropDialogOpen}
              sourceUrl={rawCropSourceUrl ?? previewLogoUrl}
              isRu={isRu}
              onClose={() => setCropDialogOpen(false)}
              onApply={handleApplyCroppedLogo}
            />
          </TabsContent>
        )}

        <TabsContent value="general" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>{isRu ? 'Панель и локализация' : 'Panel & Localization'}</CardTitle>
              <CardDescription>
                {isRu
                  ? 'Настройка названия сайта, часового пояса, языка интерфейса (RU/EN) и брендинга.'
                  : 'Configure site metadata, timezone, language (en/ru), and white-label branding.'}
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid gap-2 sm:grid-cols-2">
                <div className="space-y-1.5">
                  <Label>{isRu ? 'Название сайта' : 'Site Name'}</Label>
                  <Input
                    value={formSettings.site_name ?? ''}
                    onChange={(e) => setSetting('site_name', e.target.value)}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>{isRu ? 'Название компании / бренда' : 'Company / Brand Name'}</Label>
                  <Input
                    value={formSettings.company_name ?? ''}
                    onChange={(e) => setSetting('company_name', e.target.value)}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>{isRu ? 'Часовой пояс (IANA)' : 'Timezone (IANA)'}</Label>
                  <Input
                    placeholder="UTC, Europe/Moscow, America/New_York"
                    value={formSettings.timezone ?? timezone}
                    onChange={(e) => setSetting('timezone', e.target.value)}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>{isRu ? 'Язык интерфейса' : 'UI Language'}</Label>
                  <Select value={language} onValueChange={(v) => setLanguage(v as Language)}>
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {LANGUAGES.map((l) => (
                        <SelectItem key={l.code} value={l.code}>
                          {l.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                  <Label>{isRu ? 'White-Label режим' : 'White-Label Enabled'}</Label>
                  <Switch
                    checked={formSettings.white_label_enabled === 'true'}
                    onCheckedChange={(v) => setSetting('white_label_enabled', String(v))}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>{isRu ? 'Текст в подвале (Footer)' : 'Footer Text'}</Label>
                  <Input
                    value={formSettings.footer_text ?? ''}
                    onChange={(e) => setSetting('footer_text', e.target.value)}
                  />
                </div>
              </div>
              {isSuperAdmin && (
                <div className="flex justify-end">
                  <Button onClick={() => saveSettingsMut.mutate()} loading={saveSettingsMut.isPending}>
                    {isRu ? 'Сохранить настройки' : 'Save Settings'}
                  </Button>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

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

        {/* Telegram Tab */}
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

        {/* Core Updates Tab */}
        <TabsContent value="cores">
          <Card>
            <CardHeader className="flex flex-wrap flex-row items-center justify-between gap-3">
              <div>
                <CardTitle>
                  {isRu ? 'Каталог и обновление ядер' : 'Core Auto-Update Catalog'}
                </CardTitle>
                <CardDescription>
                  {isRu
                    ? 'Обновление прокси-ядер на активных нодах с проверкой SHA-256.'
                    : 'SHA-256 verified proxy core updates across connected nodes.'}
                </CardDescription>
              </div>
              <Button size="sm" variant="outline" onClick={handleRefreshCores} loading={refreshingCores}>
                <RefreshCw className="h-3.5 w-3.5" />
                {isRu ? 'Проверить обновления' : 'Check Updates'}
              </Button>
            </CardHeader>
            <CardContent className="space-y-4">
              {coreCatalog.length > 0 && (
                <div className="grid gap-2 sm:grid-cols-2">
                  {coreCatalog.map((c) => (
                    <div key={c.core} className="rack-inset rounded-xl p-2 text-xs space-y-1">
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-bold">{c.name || c.core.toUpperCase()}</span>
                        {c.releaseUrl && (
                          <a
                            href={c.releaseUrl}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="inline-flex items-center gap-1 text-[11px] text-primary hover:underline"
                          >
                            GitHub <ExternalLink className="h-3 w-3" />
                          </a>
                        )}
                      </div>
                      <div>
                        {isRu ? 'Последний релиз:' : 'Latest:'}{' '}
                        <span className="font-mono font-semibold text-emerald-400">
                          {c.latestVersion || (isRu ? 'недоступно' : 'unavailable')}
                        </span>
                      </div>
                      {c.sha256 && (
                        <div className="font-mono text-[11px] text-muted-foreground break-all">
                          SHA-256: {c.sha256}
                        </div>
                      )}
                      {c.error && (
                        <div className="text-[11px] text-amber-400">{c.error}</div>
                      )}
                    </div>
                  ))}
                </div>
              )}
              <div className="space-y-2.5">
                {nodes.map((n) => {
                  const nodeBusy = updatingCoreKey?.startsWith(`${n.id}:`) ?? false;
                  const coreButtons: Array<{ core: 'xray' | 'singbox' | 'naive' | 'mieru' | 'telemt'; label: string }> = [
                    { core: 'xray', label: 'Xray' },
                    { core: 'singbox', label: 'Sing-box' },
                    { core: 'naive', label: 'NaiveProxy' },
                    { core: 'mieru', label: 'Mieru' },
                    { core: 'telemt', label: 'telemt' },
                  ];
                  return (
                    <div
                      key={n.id}
                      className="node-blade relative flex flex-wrap items-center justify-between gap-3 overflow-hidden rounded-xl border border-border/80 p-2 pl-5 text-xs"
                    >
                      <div className="absolute inset-y-0 left-0 w-1 bg-emerald-400" aria-hidden="true" />
                      <div className="space-y-1">
                        <div className="flex items-center gap-2">
                          <span className="font-bold text-sm">{n.name}</span>
                          <Badge variant={n.status === 'ONLINE' ? 'success' : 'secondary'}>
                            {n.status}
                          </Badge>
                        </div>
                        <div className="font-mono text-[11px] text-muted-foreground">
                          Xray: {n.xrayVersion || '—'} · Sing-box: {n.singboxVersion || '—'} · Naive:{' '}
                          {n.naiveVersion || '—'} · Mieru: {n.mieruVersion || '—'} · telemt: {n.telemtVersion || '—'}
                        </div>
                      </div>
                      <div className="flex flex-wrap gap-1.5">
                        {coreButtons.map(({ core, label }) => {
                          const key = `${n.id}:${core}`;
                          return (
                            <Button
                              key={core}
                              size="sm"
                              variant="outline"
                              loading={updatingCoreKey === key}
                              disabled={nodeBusy || n.status === 'OFFLINE'}
                              onClick={() => updateCoreMut.mutate({ nodeId: n.id, core })}
                            >
                              <Download className="h-3.5 w-3.5" />
                              {isRu ? `Обновить ${label}` : `Update ${label}`}
                            </Button>
                          );
                        })}
                      </div>
                    </div>
                  );
                })}
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Backup Tab */}
        <TabsContent value="backup">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <CardTitle>{isRu ? 'Последние резервные копии' : 'Recent Backups'}</CardTitle>
              <Button size="sm" onClick={() => triggerBackupMut.mutate()} loading={triggerBackupMut.isPending}>
                {isRu ? 'Создать бэкап БД' : 'Trigger Database Backup'}
              </Button>
            </CardHeader>
            <CardContent className="space-y-2">
              {backupLogs.length === 0 ? (
                <RackEmptyState
                  icon={HardDrive}
                  title={isRu ? 'Резервных копий пока нет' : 'No backups logged yet'}
                />
              ) : (
                backupLogs.map((b) => {
                  const isDone = b.status === 'SUCCESS' || b.status === 'COMPLETED';
                  return (
                    <div
                      key={b.id}
                      className="rack-inset flex items-center justify-between rounded-xl px-2.5 py-1.5 text-xs"
                    >
                      <div>
                        <span className="font-semibold">{b.type}</span> · {b.destination} ·{' '}
                        <Badge variant={isDone ? 'success' : b.status === 'FAILED' ? 'destructive' : 'secondary'}>
                          {b.status}
                        </Badge>
                      </div>
                      {isDone && (
                        <a
                          href={backupApi.downloadUrl(b.id)}
                          className="inline-flex items-center gap-1 text-primary hover:underline"
                        >
                          <Download className="h-3.5 w-3.5" /> {isRu ? 'Скачать' : 'Download'}
                        </a>
                      )}
                    </div>
                  );
                })
              )}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

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

      {/* Webhook Modal */}
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
    </div>
  );
}
