import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import {
  Crop,
  ExternalLink,
  Trash2,
  ImagePlus,
  Palette,
  RotateCcw,
  Settings,
  Upload,
} from 'lucide-react';
import { BrandMark, DEFAULT_PANEL_NAME } from '@/components/branding/BrandLogo';
import { LogoCropDialog } from '@/components/branding/LogoCropDialog';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { PageHero } from '@/components/ui/PagePrimitives';
import { LANGUAGES } from '@/i18n/translations';
import { brandingApi, getApiErrorMessage } from '@/lib/api';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore, usePublicConfigStore, useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { Language } from '@/types';
import { useSettingsForm } from './settings/use-settings-form';
import { SettingsSecurityTab } from './settings/tabs/SecurityTab';
import { SettingsApiKeysTab } from './settings/tabs/ApiKeysTab';
import { SettingsWebhooksTab } from './settings/tabs/WebhooksTab';
import { SettingsCoresTab } from './settings/tabs/CoresTab';
import { SettingsBackupTab } from './settings/tabs/BackupTab';
import { SettingsTelegramTab } from './settings/tabs/TelegramTab';

export function SettingsPage() {
  const { t, language, setLanguage } = useTranslation();
  const qc = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();
  const activeTab = searchParams.get('tab') || 'general';

  const user = useAuthStore((s) => s.user);
  const timezone = useUiStore((s) => s.timezone);
  const healthVersion = usePublicConfigStore((s) => s.healthVersion);
  const branding = usePublicConfigStore((s) => s.branding);
  const setBranding = usePublicConfigStore((s) => s.setBranding);

  const isSuperAdmin = user?.role === 'SUPER_ADMIN';
  const isAdmin = user?.role === 'SUPER_ADMIN' || user?.role === 'ADMIN';
  const canEditBranding = isAdmin;
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

  // --- General & Telegram Settings (single shared form state)
  const { formSettings, setSetting, saveSettingsMut } = useSettingsForm(isSuperAdmin, isRu);

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
          setSearchParams({ tab });
        }}
      >
        <TabsList className="flex-wrap">
          <TabsTrigger value="general">{t('settings.general')}</TabsTrigger>
          {canEditBranding && <TabsTrigger value="branding">{t('settings.branding')}</TabsTrigger>}
          <TabsTrigger value="security">{t('settings.security')}</TabsTrigger>
          {isAdmin && <TabsTrigger value="api-keys">{t('settings.apiKeys')}</TabsTrigger>}
          {isAdmin && <TabsTrigger value="webhooks">{t('settings.webhooks')}</TabsTrigger>}
          {isSuperAdmin && <TabsTrigger value="telegram">{t('settings.telegram')}</TabsTrigger>}
          {isAdmin && <TabsTrigger value="cores">{t('settings.cores')}</TabsTrigger>}
          {isSuperAdmin && <TabsTrigger value="backup">{t('settings.backup')}</TabsTrigger>}
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

        {activeTab === 'security' && <SettingsSecurityTab />}
        {isAdmin && activeTab === 'api-keys' && <SettingsApiKeysTab />}
        {isAdmin && activeTab === 'webhooks' && <SettingsWebhooksTab />}
        {isSuperAdmin && activeTab === 'telegram' && (
          <SettingsTelegramTab
            formSettings={formSettings}
            setSetting={setSetting}
            onSave={() => saveSettingsMut.mutate()}
            saving={saveSettingsMut.isPending}
          />
        )}
        {isAdmin && activeTab === 'cores' && <SettingsCoresTab />}
        {isSuperAdmin && activeTab === 'backup' && <SettingsBackupTab />}

      </Tabs>

    </div>
  );
}
