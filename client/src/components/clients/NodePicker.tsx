import { useMemo, useState } from 'react';
import { Globe, Search, Server } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import type { NodeItem } from '@/types';

interface NodePickerProps {
  nodes: NodeItem[];
  allNodes: boolean;
  selectedNodeIds: string[];
  onAllNodesChange?: (allNodes: boolean) => void;
  onSelectedNodeIdsChange: (nodeIds: string[]) => void;
  hideAllNodesToggle?: boolean;
  emptyWarning?: string;
}

export function NodePicker({
  nodes,
  allNodes,
  selectedNodeIds,
  onAllNodesChange,
  onSelectedNodeIdsChange,
  hideAllNodesToggle = false,
  emptyWarning,
}: NodePickerProps) {
  const isRu = useUiStore((s) => s.language) === 'ru';
  const [search, setSearch] = useState('');

  const filteredNodes = useMemo(() => {
    const q = search.trim().toLowerCase();
    const sorted = [...nodes].sort((a, b) => a.name.localeCompare(b.name));
    if (!q) return sorted;
    return sorted.filter((n) => {
      const hay = [n.name, n.host, n.country ?? '', n.city ?? '', ...(n.tags ?? [])]
        .join(' ')
        .toLowerCase();
      return hay.includes(q);
    });
  }, [nodes, search]);

  const toggleNode = (id: string) => {
    if (selectedNodeIds.includes(id)) {
      onSelectedNodeIdsChange(selectedNodeIds.filter((nid) => nid !== id));
    } else {
      onSelectedNodeIdsChange([...selectedNodeIds, id]);
    }
  };

  const selectVisible = () => {
    const set = new Set(selectedNodeIds);
    for (const n of filteredNodes) set.add(n.id);
    onSelectedNodeIdsChange(Array.from(set));
  };

  const clearSelected = () => {
    onSelectedNodeIdsChange([]);
  };

  const showList = hideAllNodesToggle || !allNodes;
  const warningText =
    emptyWarning ??
    (isRu
      ? 'Выберите хотя бы одну ноду, чтобы клиент получил конфиги подключения.'
      : 'Select at least one node so this client receives proxy configs.');

  return (
    <div className="space-y-2.5">
      {!hideAllNodesToggle && (
        <div className="grid grid-cols-2 gap-2">
          <button
            type="button"
            onClick={() => onAllNodesChange?.(true)}
            className={cn(
              'flex items-center justify-center gap-2 rounded-lg border px-3 py-2 text-xs font-medium transition-colors',
              allNodes
                ? 'border-primary bg-primary/15 text-primary'
                : 'border-border hover:bg-accent/40 text-muted-foreground',
            )}
          >
            <Globe className="h-3.5 w-3.5" />
            {isRu ? `Все ноды (${nodes.length})` : `All Nodes (${nodes.length})`}
          </button>
          <button
            type="button"
            onClick={() => onAllNodesChange?.(false)}
            className={cn(
              'flex items-center justify-center gap-2 rounded-lg border px-3 py-2 text-xs font-medium transition-colors',
              !allNodes
                ? 'border-primary bg-primary/15 text-primary'
                : 'border-border hover:bg-accent/40 text-muted-foreground',
            )}
          >
            <Server className="h-3.5 w-3.5" />
            {isRu ? `Выбранные ноды (${selectedNodeIds.length})` : `Specific Nodes (${selectedNodeIds.length})`}
          </button>
        </div>
      )}

      {showList ? (
        <div className="space-y-2 rounded-lg border border-border/80 p-2.5 bg-background/60">
          <div className="flex flex-wrap items-center gap-2">
            <div className="relative flex-1 min-w-[160px]">
              <Search className="absolute left-2.5 top-2 h-3.5 w-3.5 text-muted-foreground" />
              <Input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder={
                  isRu
                    ? 'Поиск по имени, домену, стране, тегу...'
                    : 'Search node by name, host, country, tag...'
                }
                className="h-8 pl-8 text-xs"
              />
            </div>
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="h-8 text-xs"
              disabled={filteredNodes.length === 0}
              onClick={selectVisible}
            >
              {isRu ? `Выбрать показанные (${filteredNodes.length})` : `Select visible (${filteredNodes.length})`}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-8 text-xs"
              disabled={selectedNodeIds.length === 0}
              onClick={clearSelected}
            >
              {isRu ? 'Очистить' : 'Clear'}
            </Button>
          </div>

          <div className="max-h-56 overflow-y-auto space-y-1 pr-1">
            {filteredNodes.length === 0 ? (
              <div className="py-5 text-center text-xs text-muted-foreground">
                {isRu ? `По запросу «${search}» ноды не найдены.` : `No nodes match "${search}".`}
              </div>
            ) : (
              filteredNodes.map((n) => {
                const checked = selectedNodeIds.includes(n.id);
                const inboundCount = n._count?.inbounds ?? n.inbounds?.length;
                return (
                  <label
                    key={n.id}
                    className={cn(
                      'flex cursor-pointer items-center justify-between gap-2 rounded-md border px-2.5 py-1.5 text-xs transition-colors hover:bg-accent/40',
                      checked ? 'border-primary/40 bg-primary/10' : 'border-transparent',
                    )}
                  >
                    <div className="flex items-center gap-2 min-w-0">
                      <Checkbox checked={checked} onCheckedChange={() => toggleNode(n.id)} />
                      <span
                        className={cn(
                          'h-2 w-2 shrink-0 rounded-full',
                          n.status === 'ONLINE'
                            ? 'bg-emerald-400'
                            : n.status === 'MAINTENANCE'
                              ? 'bg-amber-400'
                              : 'bg-zinc-500',
                        )}
                      />
                      <span className="font-medium break-words min-w-0">{n.name}</span>
                      {n.country && (
                        <Badge variant="outline" className="px-1.5 py-0 text-[10px]">
                          {n.country}
                        </Badge>
                      )}
                    </div>
                    <div className="flex items-center gap-2 shrink-0 text-[11px] text-muted-foreground">
                      <span className="font-mono break-words min-w-0">{n.host}</span>
                      {inboundCount !== undefined && (
                        <Badge variant="secondary" className="px-1.5 py-0 text-[10px]">
                          {inboundCount} {isRu ? 'прот.' : 'ib'}
                        </Badge>
                      )}
                    </div>
                  </label>
                );
              })
            )}
          </div>

          {selectedNodeIds.length === 0 && (
            <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-2.5 py-1.5 text-[11px] text-amber-600 dark:text-amber-400">
              {warningText}
            </div>
          )}
        </div>
      ) : (
        <div className="rounded-lg border border-dashed px-3 py-2 text-xs text-muted-foreground">
          {isRu
            ? `Клиент автоматически получит доступ ко всем активным нодам (сейчас ${nodes.length}), включая будущие.`
            : `Client will automatically have access to all enabled nodes (${nodes.length} total), including any newly added nodes.`}
        </div>
      )}
    </div>
  );
}
