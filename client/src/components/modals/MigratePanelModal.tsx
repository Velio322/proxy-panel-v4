import { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { JsonEditor } from '@/components/JsonEditor';
import { clientsApi, getApiErrorMessage } from '@/lib/api';
import { toast } from '@/hooks/use-toast';

interface MigratePanelModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onMigrated: () => void;
}

export function MigratePanelModal({ open, onOpenChange, onMigrated }: MigratePanelModalProps) {
  const [source, setSource] = useState<'marzban' | 'hiddify'>('marzban');
  const [content, setContent] = useState('{\n  "users": []\n}');
  const [errors, setErrors] = useState<string[]>([]);
  const [preview, setPreview] = useState<{ created: number; skipped: number; errors?: string[] } | null>(
    null,
  );
  const [running, setRunning] = useState(false);

  const handleRun = async (dryRun: boolean) => {
    if (errors.length > 0) return;
    setRunning(true);
    try {
      const parsed = JSON.parse(content);
      const res = await clientsApi.migrate({ source, data: parsed, dryRun });
      setPreview(res);
      if (!dryRun) {
        toast({
          title: `Migrated ${res.created} clients (${res.skipped} skipped)`,
          variant: 'success',
        });
        onMigrated();
        onOpenChange(false);
      }
    } catch (err) {
      toast({ title: 'Migration failed', description: getApiErrorMessage(err), variant: 'destructive' });
    } finally {
      setRunning(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Migrate from Marzban / Hiddify</DialogTitle>
          <DialogDescription>
            Paste or upload a Marzban or Hiddify JSON export, run a dry-run preview, then apply.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          <div className="w-48 space-y-1.5">
            <Label>Source Panel</Label>
            <Select value={source} onValueChange={(v) => setSource(v as 'marzban' | 'hiddify')}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="marzban">Marzban JSON</SelectItem>
                <SelectItem value="hiddify">Hiddify JSON</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <JsonEditor
            value={content}
            onChange={setContent}
            language="json"
            onValidationChange={setErrors}
            height="240px"
          />

          {preview && (
            <div className="rounded-lg border bg-muted/20 p-3 text-xs space-y-1">
              <div>
                Dry-run result: <strong>{preview.created}</strong> created,{' '}
                <strong>{preview.skipped}</strong> skipped
              </div>
              {preview.errors?.length ? (
                <div className="text-red-400">{preview.errors.join('; ')}</div>
              ) : null}
            </div>
          )}
        </div>

        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            type="button"
            variant="secondary"
            onClick={() => handleRun(true)}
            loading={running}
            disabled={errors.length > 0}
          >
            Dry Run
          </Button>
          <Button
            type="button"
            onClick={() => handleRun(false)}
            loading={running}
            disabled={errors.length > 0}
          >
            Apply Import
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
