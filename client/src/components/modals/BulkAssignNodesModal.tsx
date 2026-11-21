import { useEffect, useState } from 'react';
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
import { NodePicker } from '@/components/clients/NodePicker';
import { clientsApi, getApiErrorMessage } from '@/lib/api';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { NodeItem } from '@/types';

type BulkNodeMode = 'replace' | 'add' | 'remove' | 'all';

interface BulkAssignNodesModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  clientIds: string[];
  nodes: NodeItem[];
  onApplied: () => void;
}

export function BulkAssignNodesModal({
  open,
  onOpenChange,
  clientIds,
  nodes,
  onApplied,
}: BulkAssignNodesModalProps) {
  const isRu = useUiStore((s) => s.language) === 'ru';
  const [mode, setMode] = useState<BulkNodeMode>('replace');
  const [selectedNodeIds, setSelectedNodeIds] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) {
      setMode('replace');
      setSelectedNodeIds([]);
    }
  }, [open]);

  const handleApply = async () => {
    if (clientIds.length === 0) return;
    if ((mode === 'add' || mode === 'remove' || mode === 'replace') && selectedNodeIds.length === 0) {
      toast({
        title:
          mode === 'replace'
            ? isRu
              ? 'Выберите ноды или режим «Разрешить все ноды»'
              : 'Select nodes or switch mode to Allow All Nodes'
            : isRu
              ? 'Выберите хотя бы одну ноду'
              : 'Select at least one node',
        variant: 'destructive',
      });
      return;
    }

    setSaving(true);
    try {
      const out = await clientsApi.bulkAction({
        action: 'assign_nodes',
        clientIds,
        nodeMode: mode,
        nodeIds: mode === 'all' ? [] : selectedNodeIds,
        allNodes: mode === 'all',
      });
      toast({
        title: isRu ? `Обновлены ноды у клиентов: ${out.affected}` : `Updated nodes for ${out.affected} client(s)`,
        variant: 'success',
      });
      onApplied();
      onOpenChange(false);
    } catch (err) {
      toast({
        title: isRu ? 'Ошибка назначения нод' : 'Bulk node assignment failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>
            {isRu ? `Назначить ноды (клиентов: ${clientIds.length})` : `Assign Nodes (${clientIds.length} client(s))`}
          </DialogTitle>
          <DialogDescription>
            {isRu
              ? 'Привяжите выбранных клиентов к конкретным нодам или откройте доступ ко всем нодам.'
              : 'Pin selected clients to specific nodes or reset them to All Nodes. Matching inbounds are synced automatically.'}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-1.5">
            <Label>{isRu ? 'Режим изменения' : 'Assignment Mode'}</Label>
            <Select value={mode} onValueChange={(v) => setMode(v as BulkNodeMode)}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="replace">
                  {isRu ? 'Заменить список нод (установить точно эти)' : 'Replace pinned nodes (set exact list)'}
                </SelectItem>
                <SelectItem value="add">
                  {isRu ? 'Добавить ноды' : 'Add nodes (pin if currently All Nodes)'}
                </SelectItem>
                <SelectItem value="remove">
                  {isRu ? 'Убрать ноды' : 'Remove nodes (unpin from chosen nodes)'}
                </SelectItem>
                <SelectItem value="all">
                  {isRu ? 'Разрешить все ноды (снять привязку)' : 'Allow All Nodes (unpin)'}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          {mode === 'all' ? (
            <div className="rounded-lg border border-dashed p-3 text-xs text-muted-foreground">
              {isRu
                ? 'Выбранным клиентам будут доступны все активные ноды и протоколы.'
                : 'Selected clients will be switched to All Nodes and receive enabled inbounds across every node matching their protocols.'}
            </div>
          ) : (
            <NodePicker
              nodes={nodes}
              allNodes={false}
              hideAllNodesToggle
              selectedNodeIds={selectedNodeIds}
              onSelectedNodeIdsChange={setSelectedNodeIds}
              emptyWarning={isRu ? 'Выберите одну или несколько целевых нод.' : 'Select one or more target nodes.'}
            />
          )}
        </div>

        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            {isRu ? 'Отмена' : 'Cancel'}
          </Button>
          <Button type="button" onClick={handleApply} loading={saving}>
            {isRu ? `Применить (${clientIds.length})` : `Apply to ${clientIds.length} Client(s)`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
