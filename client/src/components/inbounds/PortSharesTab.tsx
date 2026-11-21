import { useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Badge } from '@/components/ui/badge';
import { PROTOCOLS } from '@/lib/core-utils';
import type { Protocol } from '@/types';
import type { PortShareFormState } from '@/components/inbounds/types';

interface PortSharesTabProps {
  shares: PortShareFormState[];
  onChange: (next: PortShareFormState[]) => void;
}

export function PortSharesTab({ shares, onChange }: PortSharesTabProps) {
  const [protocol, setProtocol] = useState<Protocol>('VLESS');
  const [tag, setTag] = useState('');
  const [host, setHost] = useState('');
  const [path, setPath] = useState('/');

  const handleAdd = () => {
    if (!tag.trim()) return;
    onChange([
      ...shares,
      {
        protocol,
        tag: tag.trim(),
        host: host.trim(),
        path: path.trim() || '/',
        enable: true,
        settings: {},
        stream: {},
      },
    ]);
    setTag('');
    setHost('');
    setPath('/');
  };

  return (
    <div className="space-y-4">
      <div className="rounded-lg border bg-muted/20 p-3 space-y-3">
        <div className="text-xs font-medium text-muted-foreground">
          Add SNI / Path PortShare fallback on the same listening port
        </div>
        <div className="grid gap-2 sm:grid-cols-4">
          <div className="space-y-1">
            <Label>Protocol</Label>
            <Select value={protocol} onValueChange={(v) => setProtocol(v as Protocol)}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {PROTOCOLS.map((p) => (
                  <SelectItem key={p} value={p}>
                    {p}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label>Tag</Label>
            <Input
              placeholder="share-ws"
              value={tag}
              onChange={(e) => setTag(e.target.value)}
            />
          </div>
          <div className="space-y-1">
            <Label>SNI / Host</Label>
            <Input
              placeholder="cdn.example.com"
              value={host}
              onChange={(e) => setHost(e.target.value)}
            />
          </div>
          <div className="space-y-1">
            <Label>Path</Label>
            <div className="flex gap-1.5">
              <Input
                placeholder="/ws"
                value={path}
                onChange={(e) => setPath(e.target.value)}
              />
              <Button type="button" size="sm" onClick={handleAdd} disabled={!tag.trim()}>
                <Plus className="h-4 w-4" />
              </Button>
            </div>
          </div>
        </div>
      </div>

      {shares.length === 0 ? (
        <div className="rounded-lg border border-dashed py-6 text-center text-xs text-muted-foreground">
          No PortShares configured for this inbound.
        </div>
      ) : (
        <div className="space-y-2">
          {shares.map((share, idx) => (
            <div
              key={share.id || `${share.tag}-${idx}`}
              className="flex flex-wrap items-center justify-between gap-2 rounded-lg border p-3"
            >
              <div className="flex items-center gap-2">
                <Badge variant="outline">{share.protocol}</Badge>
                <span className="text-sm font-medium">{share.tag}</span>
                {share.host && <span className="text-xs text-muted-foreground">{share.host}</span>}
                {share.path && <span className="text-xs font-mono text-muted-foreground">{share.path}</span>}
              </div>
              <div className="flex items-center gap-2">
                <Switch
                  checked={share.enable}
                  onCheckedChange={(v) => {
                    const next = [...shares];
                    next[idx] = { ...share, enable: v };
                    onChange(next);
                  }}
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  aria-label={`Remove port share ${share.tag}`}
                  onClick={() => onChange(shares.filter((_, i) => i !== idx))}
                >
                  <Trash2 className="h-4 w-4 text-red-400" />
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
