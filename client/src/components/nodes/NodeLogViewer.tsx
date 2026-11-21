import { useEffect, useMemo, useRef, useState } from 'react';
import { Download, Pause, Play, Search, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { nodesApi } from '@/lib/api';
import { useUiStore } from '@/lib/store';
import type { NodeLogEntry } from '@/types';

interface NodeLogViewerProps {
  nodeId: string;
}

const CORES = ['all', 'xray', 'singbox', 'naive', 'mieru', 'agent'] as const;

export function NodeLogViewer({ nodeId }: NodeLogViewerProps) {
  const isRu = useUiStore((s) => s.language === 'ru');
  const [coreFilter, setCoreFilter] = useState<string>('all');
  const [streamFilter, setStreamFilter] = useState<'all' | 'stdout' | 'stderr'>('all');
  const [search, setSearch] = useState('');
  const [paused, setPaused] = useState(false);
  const pausedRef = useRef(false);
  pausedRef.current = paused;
  const [mode, setMode] = useState<'stream' | 'polling'>('stream');
  const [logs, setLogs] = useState<NodeLogEntry[]>([]);
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!nodeId) return;
    let active = true;
    let pollTimer: number | undefined;
    let es: EventSource | null = null;

    const fetchOnce = async () => {
      if (pausedRef.current || (typeof document !== 'undefined' && document.hidden)) return;
      try {
        const items = await nodesApi.getLogs(nodeId, {
          core: coreFilter === 'all' ? undefined : coreFilter,
          limit: 200,
        });
        if (active && !pausedRef.current) {
          setLogs(items.slice(-300));
        }
      } catch {
        // ignore transient poll errors
      }
    };

    const startPolling = () => {
      if (!active) return;
      setMode('polling');
      fetchOnce();
      pollTimer = window.setInterval(fetchOnce, 4000);
    };

    fetchOnce();

    if (typeof EventSource !== 'undefined') {
      const qs = new URLSearchParams({ nodeId });
      if (coreFilter !== 'all') qs.set('core', coreFilter);
      const url = `/api/v1/logs/stream?${qs.toString()}`;
      try {
        es = new EventSource(url, { withCredentials: true });
        setMode('stream');
        es.onmessage = (evt) => {
          if (!active || pausedRef.current) return;
          try {
            const parsed = JSON.parse(evt.data) as NodeLogEntry;
            setLogs((prev) => [...prev.slice(-299), parsed]);
          } catch {
            setLogs((prev) => [
              ...prev.slice(-299),
              {
                timestamp: new Date().toISOString(),
                core: coreFilter === 'all' ? 'agent' : coreFilter,
                level: 'INFO',
                message: String(evt.data),
              },
            ]);
          }
        };
        es.onerror = () => {
          es?.close();
          es = null;
          if (!pollTimer) startPolling();
        };
      } catch {
        startPolling();
      }
    } else {
      startPolling();
    }

    return () => {
      active = false;
      es?.close();
      if (pollTimer) window.clearInterval(pollTimer);
    };
  }, [nodeId, coreFilter]);

  const filteredLogs = useMemo(() => {
    const q = search.trim().toLowerCase();
    return logs.filter((l) => {
      if (coreFilter !== 'all' && l.core.toLowerCase() !== coreFilter) return false;
      if (streamFilter !== 'all' && (l.stream || 'stdout') !== streamFilter) return false;
      if (q && !l.message.toLowerCase().includes(q) && !l.level.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [logs, coreFilter, streamFilter, search]);

  useEffect(() => {
    if (!paused && containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [filteredLogs.length, paused]);

  const handleDownload = () => {
    const text = filteredLogs
      .map((l) => `[${l.timestamp}] [${l.core}] [${l.level}] ${l.message}`)
      .join('\n');
    const blob = new Blob([text], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `node-${nodeId}.log`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-1.5">
          {CORES.map((c) => (
            <Button
              key={c}
              type="button"
              size="sm"
              variant={coreFilter === c ? 'default' : 'outline'}
              onClick={() => setCoreFilter(c)}
              className="h-7 px-2.5 text-xs uppercase"
            >
              {c === 'all' ? (isRu ? 'Все' : 'All') : c}
            </Button>
          ))}
        </div>
        <div className="flex items-center gap-2">
          <Badge variant={mode === 'stream' ? 'success' : 'secondary'}>
            {mode === 'stream'
              ? isRu
                ? 'Поток'
                : 'Live Stream'
              : isRu
                ? 'Опрос (4с)'
                : 'Polling (4s)'}
          </Badge>
          {(['all', 'stdout', 'stderr'] as const).map((st) => (
            <Button
              key={st}
              type="button"
              size="sm"
              variant={streamFilter === st ? 'secondary' : 'ghost'}
              onClick={() => setStreamFilter(st)}
              className="h-7 px-2 text-xs"
            >
              {st === 'all' ? (isRu ? 'Все' : 'all') : st}
            </Button>
          ))}
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <div className="relative flex-1 min-w-[180px]">
          <Search className="absolute left-3 top-2.5 h-3.5 w-3.5 text-muted-foreground" />
          <Input
            placeholder={isRu ? 'Поиск по логам...' : 'Filter logs...'}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="h-8 pl-8 text-xs"
          />
        </div>
        <Button type="button" size="sm" variant="outline" onClick={() => setPaused((p) => !p)}>
          {paused ? <Play className="h-3.5 w-3.5" /> : <Pause className="h-3.5 w-3.5" />}
          {paused ? (isRu ? 'Продолжить' : 'Resume') : isRu ? 'Пауза' : 'Pause'}
        </Button>
        <Button type="button" size="sm" variant="outline" onClick={() => setLogs([])}>
          <Trash2 className="h-3.5 w-3.5" /> {isRu ? 'Очистить' : 'Clear'}
        </Button>
        <Button type="button" size="sm" variant="outline" onClick={handleDownload}>
          <Download className="h-3.5 w-3.5" /> .log
        </Button>
      </div>

      <div className="overflow-hidden rounded-lg border bg-zinc-950">
        <div className="flex items-center justify-between border-b border-zinc-800/80 bg-zinc-900/70 px-3 py-1.5 text-[11px] font-mono text-zinc-400">
          <div className="flex items-center gap-2">
            <span
              className={
                paused ? 'h-2 w-2 rounded-full bg-amber-400' : 'h-2 w-2 rounded-full bg-emerald-400 animate-pulse'
              }
            />
            <span>{paused ? (isRu ? 'Пауза' : 'Paused') : isRu ? 'Монитор вывода' : 'Output Stream'}</span>
          </div>
          <span>
            {filteredLogs.length} {isRu ? 'строк' : 'lines'}
          </span>
        </div>
        <div
          ref={containerRef}
          className="h-80 overflow-y-auto p-3 font-mono text-xs text-zinc-200 space-y-1"
        >
          {filteredLogs.length === 0 ? (
            <div className="py-12 text-center text-zinc-500">
              {isRu ? 'Логи пока отсутствуют.' : 'No log lines available.'}
            </div>
          ) : (
            filteredLogs.map((l, idx) => (
              <div key={idx} className="leading-relaxed break-all">
                <span className="text-zinc-500">{l.timestamp}</span>{' '}
                <span className="text-sky-400">[{l.core}]</span>{' '}
                <span
                  className={
                    l.level.toUpperCase().includes('ERR')
                      ? 'text-red-400'
                      : l.level.toUpperCase().includes('WARN')
                        ? 'text-amber-400'
                        : 'text-emerald-400'
                  }
                >
                  {l.level}
                </span>{' '}
                <span>{l.message}</span>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
