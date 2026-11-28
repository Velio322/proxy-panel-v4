import { useMemo, useState } from 'react';
import { Upload } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Badge } from '@/components/ui/badge';
import { clientsApi, getApiErrorMessage } from '@/lib/api';
import { parseCsvClients } from '@/pages/clients/utils';
import { toast } from '@/hooks/use-toast';

interface CsvImportModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onImported: () => void;
}

const SAMPLE_CSV = `username,email,trafficLimitGb,expireDays,protocols,note
alice,alice@example.com,100,30,VLESS|HYSTERIA2,VIP user
bob,bob@example.com,50,14,VLESS,Trial`;

export function CsvImportModal({ open, onOpenChange, onImported }: CsvImportModalProps) {
  const [csv, setCsv] = useState(SAMPLE_CSV);
  const [submitting, setSubmitting] = useState(false);

  const parsed = useMemo(() => parseCsvClients(csv), [csv]);

  const handleFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    setCsv(text);
  };

  const handleImport = async () => {
    if (parsed.rows.length === 0) return;
    setSubmitting(true);
    try {
      const res = await clientsApi.importCsv({ rows: parsed.rows });
      toast({
        title: `Imported ${res.created} clients (${res.skipped} skipped)`,
        variant: 'success',
      });
      onImported();
      onOpenChange(false);
    } catch (err) {
      toast({ title: 'Import failed', description: getApiErrorMessage(err), variant: 'destructive' });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent size="2xl">
        <DialogHeader>
          <DialogTitle>Import Clients from CSV</DialogTitle>
          <DialogDescription>
            Columns: username, email, trafficLimitGb, expireDays, protocols (| separated), note, planId
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          <label className="inline-flex cursor-pointer items-center gap-2 rounded-lg border px-3 py-1.5 text-xs font-medium hover:bg-accent">
            <Upload className="h-3.5 w-3.5" />
            <span>Upload .csv file</span>
            <input type="file" accept=".csv,text/csv" className="hidden" onChange={handleFile} />
          </label>

          <Textarea
            rows={7}
            value={csv}
            onChange={(e) => setCsv(e.target.value)}
            className="font-mono text-xs"
          />

          <div className="flex items-center gap-2 text-xs">
            <Badge variant="success">{parsed.rows.length} valid rows</Badge>
            {parsed.errors.length > 0 && (
              <Badge variant="destructive">{parsed.errors.length} error(s)</Badge>
            )}
          </div>

          {parsed.errors.length > 0 && (
            <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-2 text-xs text-red-400">
              {parsed.errors.slice(0, 4).map((e, i) => (
                <div key={i}>{e}</div>
              ))}
            </div>
          )}
        </div>

        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            type="button"
            onClick={handleImport}
            loading={submitting}
            disabled={parsed.rows.length === 0 || parsed.errors.length > 0}
          >
            Import {parsed.rows.length} Clients
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
