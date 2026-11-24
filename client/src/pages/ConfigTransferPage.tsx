import { useState } from 'react';
import { ArrowRightLeft, Download, FileCode2, Play, ShieldCheck, Upload } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { PageHero } from '@/components/ui/PagePrimitives';
import { JsonEditor } from '@/components/JsonEditor';
import { configTransferApi, getApiErrorMessage } from '@/lib/api';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';

export function ConfigTransferPage() {
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const [exportVersion, setExportVersion] = useState<'v1' | 'v2'>('v2');
  const [exportFormat, setExportFormat] = useState<'json' | 'yaml'>('json');
  const [includeSecrets, setIncludeSecrets] = useState(false);
  const [exportedText, setExportedText] = useState('');

  const [importVersion, setImportVersion] = useState<'v1' | 'v2'>('v2');
  const [importFormat, setImportFormat] = useState<'json' | 'yaml'>('json');
  const [importMode, setImportMode] = useState<'create-only' | 'upsert'>('upsert');
  const [importText, setImportText] = useState('{\n  "version": 2,\n  "nodes": [],\n  "inbounds": []\n}');
  const [importErrors, setImportErrors] = useState<string[]>([]);
  const [importResult, setImportResult] = useState<{
    created?: number;
    updated?: number;
    skipped?: number;
    errors?: string[];
  } | null>(null);
  const [busy, setBusy] = useState(false);

  const handleExport = async () => {
    setBusy(true);
    try {
      const data = await configTransferApi.exportConfig({
        version: exportVersion,
        format: exportFormat,
        includeSecrets,
      });
      setExportedText(data);
    } catch (err) {
      toast({
        title: isRu ? 'Ошибка экспорта' : 'Export failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      });
    } finally {
      setBusy(false);
    }
  };

  const handleDownloadExport = () => {
    if (!exportedText) return;
    const blob = new Blob([exportedText], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `mypanel3-export-${exportVersion}.${exportFormat}`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleImport = async (apply: boolean) => {
    if (importErrors.length > 0) return;
    setBusy(true);
    try {
      const res = await configTransferApi.importConfig({
        version: importVersion,
        format: importFormat,
        mode: importMode,
        apply,
        content: importText,
      });
      setImportResult(res);
      toast({
        title: apply
          ? isRu
            ? 'Импорт применён'
            : 'Import applied'
          : isRu
            ? 'Тестовый прогон (Dry-run) завершён'
            : 'Dry-run complete',
        variant: 'success',
      });
    } catch (err) {
      toast({
        title: isRu ? 'Ошибка импорта' : 'Import failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      });
    } finally {
      setBusy(false);
    }
  };

  const handleFileUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    setImportText(text);
    if (file.name.endsWith('.yaml') || file.name.endsWith('.yml')) {
      setImportFormat('yaml');
    } else {
      setImportFormat('json');
    }
  };

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Перенос конфигурации (v1 / v2)' : 'Config Transfer (v1 / v2)'}
        subtitle={
          isRu
            ? 'Экспорт, валидация, тестовый прогон (Dry-Run) и импорт топологии нод, протоколов и правил панели'
            : 'Export, validate, dry-run, and import panel topology snapshots.'
        }
        icon={<ArrowRightLeft className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs">
            JSON / YAML
          </Badge>
        }
      />

      {/* 3-Card Summary Strip */}
      <div className="grid gap-3 sm:grid-cols-3">
        <Card className="p-3 space-y-1">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Схема данных' : 'Schema Engine'}</span>
            <FileCode2 className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="font-mono text-xl font-bold">v1 / v2 Topology</div>
        </Card>
        <Card className="p-3 space-y-1">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Безопасный импорт' : 'Safe Dry-Run'}</span>
            <ShieldCheck className="h-4 w-4 text-cyan-400" />
          </div>
          <div className="font-mono text-xl font-bold">Upsert / Create-Only</div>
        </Card>
        <Card className="p-3 space-y-1">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Проверка синтаксиса' : 'Live Validation'}</span>
            <ArrowRightLeft className="h-4 w-4 text-primary" />
          </div>
          <div className="font-mono text-xl font-bold">
            {importErrors.length === 0 ? (isRu ? 'ОК' : 'Valid') : `${importErrors.length} Err`}
          </div>
        </Card>
      </div>

      <Tabs defaultValue="export">
        <TabsList>
          <TabsTrigger value="export">{isRu ? 'Экспорт' : 'Export'}</TabsTrigger>
          <TabsTrigger value="import">{isRu ? 'Импорт и Dry-Run' : 'Import & Dry-Run'}</TabsTrigger>
        </TabsList>

        <TabsContent value="export" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>{isRu ? 'Экспорт конфигурации' : 'Export Configuration'}</CardTitle>
              <CardDescription>
                {isRu
                  ? 'Сформируйте переносимый JSON или YAML снимок топологии.'
                  : 'Generate a portable JSON or YAML bundle.'}
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex flex-wrap items-end gap-4">
                <div className="w-32 space-y-1.5">
                  <Label>{isRu ? 'Версия схемы' : 'Schema Version'}</Label>
                  <Select value={exportVersion} onValueChange={(v) => setExportVersion(v as 'v1' | 'v2')}>
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="v1">v1</SelectItem>
                      <SelectItem value="v2">v2</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="w-32 space-y-1.5">
                  <Label>{isRu ? 'Формат' : 'Format'}</Label>
                  <Select value={exportFormat} onValueChange={(v) => setExportFormat(v as 'json' | 'yaml')}>
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="json">JSON</SelectItem>
                      <SelectItem value="yaml">YAML</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="flex items-center gap-2 rounded-lg border px-3 py-2">
                  <Label>{isRu ? 'Включить секреты' : 'Include Secrets'}</Label>
                  <Switch checked={includeSecrets} onCheckedChange={setIncludeSecrets} />
                </div>
                <Button onClick={handleExport} loading={busy}>
                  <Play className="h-4 w-4" />
                  <span>{isRu ? 'Сформировать экспорт' : 'Generate Export'}</span>
                </Button>
                {exportedText && (
                  <Button variant="outline" onClick={handleDownloadExport}>
                    <Download className="h-4 w-4" />
                    <span>{isRu ? 'Скачать файл' : 'Download'}</span>
                  </Button>
                )}
              </div>

              {exportedText && (
                <JsonEditor
                  value={exportedText}
                  onChange={setExportedText}
                  language={exportFormat}
                  readOnly
                  height="320px"
                />
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="import" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>{isRu ? 'Импорт конфигурации' : 'Import Configuration'}</CardTitle>
              <CardDescription>
                {isRu
                  ? 'Синтаксические ошибки автоматически блокируют применение конфигурации.'
                  : 'Syntax errors automatically block apply & dry-run.'}
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex flex-wrap items-end gap-3">
                <div className="w-28 space-y-1.5">
                  <Label>{isRu ? 'Версия' : 'Version'}</Label>
                  <Select value={importVersion} onValueChange={(v) => setImportVersion(v as 'v1' | 'v2')}>
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="v1">v1</SelectItem>
                      <SelectItem value="v2">v2</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="w-28 space-y-1.5">
                  <Label>{isRu ? 'Формат' : 'Format'}</Label>
                  <Select value={importFormat} onValueChange={(v) => setImportFormat(v as 'json' | 'yaml')}>
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="json">JSON</SelectItem>
                      <SelectItem value="yaml">YAML</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="w-36 space-y-1.5">
                  <Label>{isRu ? 'Режим' : 'Mode'}</Label>
                  <Select
                    value={importMode}
                    onValueChange={(v) => setImportMode(v as 'create-only' | 'upsert')}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="upsert">Upsert</SelectItem>
                      <SelectItem value="create-only">Create Only</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <label className="inline-flex cursor-pointer items-center gap-2 rounded-lg border px-3 py-2 text-xs font-medium hover:bg-accent">
                  <Upload className="h-3.5 w-3.5" />
                  <span>{isRu ? 'Загрузить файл' : 'Load File'}</span>
                  <input type="file" accept=".json,.yaml,.yml" className="hidden" onChange={handleFileUpload} />
                </label>
              </div>

              <JsonEditor
                value={importText}
                onChange={setImportText}
                language={importFormat}
                schemaKind="config-transfer"
                onValidationChange={setImportErrors}
                height="300px"
              />

              {importResult && (
                <div className="rack-inset rounded-lg p-3 text-xs space-y-1">
                  <div>
                    {isRu ? 'Создано:' : 'Created:'} <strong>{importResult.created ?? 0}</strong> ·{' '}
                    {isRu ? 'Обновлено:' : 'Updated:'} <strong>{importResult.updated ?? 0}</strong> ·{' '}
                    {isRu ? 'Пропущено:' : 'Skipped:'} <strong>{importResult.skipped ?? 0}</strong>
                  </div>
                  {importResult.errors?.length ? (
                    <div className="text-red-400">{importResult.errors.join('; ')}</div>
                  ) : null}
                </div>
              )}

              <div className="flex justify-end gap-2">
                <Button
                  variant="secondary"
                  disabled={importErrors.length > 0}
                  loading={busy}
                  onClick={() => handleImport(false)}
                >
                  {isRu ? 'Проверить (Dry Run)' : 'Dry Run'}
                </Button>
                <Button
                  disabled={importErrors.length > 0}
                  loading={busy}
                  onClick={() => handleImport(true)}
                >
                  {isRu ? 'Применить импорт' : 'Apply Import'}
                </Button>
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
