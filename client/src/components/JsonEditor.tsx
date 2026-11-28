import React, { useCallback, useEffect, useMemo, useRef } from 'react';
import yaml from 'js-yaml';
import { AlertCircle, Check, CheckCircle2, Code2, Copy, Minimize2, Sparkles } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/core-utils';
import { useCopyFeedback } from '@/hooks/use-copy-feedback';

export type EditorLanguage = 'json' | 'yaml' | 'plaintext';
export type JsonSchemaKind =
  | 'xray-inbound'
  | 'singbox-inbound'
  | 'routing-rule'
  | 'subscription-template'
  | 'config-transfer';

export interface JsonEditorProps {
  value: string;
  onChange: (next: string) => void;
  language?: EditorLanguage;
  schemaKind?: JsonSchemaKind;
  height?: string;
  readOnly?: boolean;
  onValidationChange?: (errors: string[]) => void;
  className?: string;
  ariaLabel?: string;
}

export function validateEditorContent(
  content: string,
  language: EditorLanguage,
  schemaKind?: JsonSchemaKind,
): string[] {
  const trimmed = content.trim();
  if (!trimmed) {
    return language === 'plaintext' ? [] : ['Content cannot be empty'];
  }
  const normalized =
    schemaKind === 'subscription-template'
      ? trimmed.replace(/"([^"\\]|\\.)*"|\{\{[^}]+\}\}/g, (m) =>
          m.startsWith('"') ? m.replace(/\{\{[^}]+\}\}/g, '__TPL__') : '"__TPL__"',
        )
      : trimmed;
  if (language === 'json') {
    try {
      const parsed = JSON.parse(normalized) as unknown;
      if (schemaKind === 'xray-inbound' || schemaKind === 'singbox-inbound') {
        if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
          return ['Inbound configuration must be a JSON object'];
        }
      }
      if (schemaKind === 'config-transfer') {
        if (!parsed || typeof parsed !== 'object') {
          return ['Config transfer payload must be a JSON object'];
        }
      }
      return [];
    } catch (err) {
      return [err instanceof Error ? err.message : 'Invalid JSON syntax'];
    }
  }
  if (language === 'yaml') {
    try {
      yaml.load(normalized);
      return [];
    } catch (err) {
      return [err instanceof Error ? err.message : 'Invalid YAML syntax'];
    }
  }
  return [];
}

const JSON_TOKEN_RE =
  /("(?:\\u[a-fA-F0-9]{4}|\\[^u]|[^\\"])*"(?:\s*:)?|\b(?:true|false|null)\b|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/g;

function highlightJsonLine(line: string): React.ReactNode {
  if (!line) return ' ';
  const nodes: React.ReactNode[] = [];
  let lastIdx = 0;
  JSON_TOKEN_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  let key = 0;
  while ((m = JSON_TOKEN_RE.exec(line)) !== null) {
    if (m.index > lastIdx) {
      nodes.push(line.slice(lastIdx, m.index));
    }
    const token = m[0];
    let cls = 'text-amber-600 dark:text-amber-300';
    if (token.startsWith('"')) {
      cls = token.endsWith(':')
        ? 'text-sky-600 dark:text-sky-300'
        : 'text-emerald-600 dark:text-emerald-300';
    } else if (/^(?:true|false|null)$/.test(token)) {
      cls = 'text-violet-600 dark:text-violet-400';
    }
    nodes.push(
      <span key={key++} className={cls}>
        {token}
      </span>,
    );
    lastIdx = m.index + token.length;
  }
  if (lastIdx < line.length) {
    nodes.push(line.slice(lastIdx));
  }
  return nodes;
}

function highlightYamlLine(line: string): React.ReactNode {
  if (!line) return ' ';
  const trimmed = line.trimStart();
  if (trimmed.startsWith('#')) {
    return <span className="text-muted-foreground italic">{line}</span>;
  }
  const colonIdx = line.indexOf(':');
  if (colonIdx > 0 && !trimmed.startsWith('"')) {
    const keyPart = line.slice(0, colonIdx + 1);
    const valPart = line.slice(colonIdx + 1);
    return (
      <>
        <span className="text-sky-600 dark:text-sky-300">{keyPart}</span>
        <span className="text-emerald-600 dark:text-emerald-300">{valPart}</span>
      </>
    );
  }
  return line;
}

export function JsonEditor({
  value,
  onChange,
  language = 'json',
  schemaKind,
  height = '320px',
  readOnly = false,
  onValidationChange,
  className,
  ariaLabel = 'Code editor',
}: JsonEditorProps) {
  const [copied, copyValue] = useCopyFeedback();
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const gutterRef = useRef<HTMLDivElement>(null);
  const highlightRef = useRef<HTMLPreElement>(null);

  const syntaxErrors = useMemo(
    () => validateEditorContent(value, language, schemaKind),
    [value, language, schemaKind],
  );

  useEffect(() => {
    onValidationChange?.(syntaxErrors);
  }, [syntaxErrors, onValidationChange]);

  const handleFormat = useCallback(() => {
    if (language === 'json') {
      try {
        const parsed = JSON.parse(value);
        onChange(JSON.stringify(parsed, null, 2));
      } catch {
        // cannot format invalid JSON
      }
    } else if (language === 'yaml') {
      try {
        const parsed = yaml.load(value);
        onChange(yaml.dump(parsed, { indent: 2 }));
      } catch {
        // cannot format invalid YAML
      }
    }
  }, [language, value, onChange]);

  const handleMinify = useCallback(() => {
    if (language !== 'json') return;
    try {
      const parsed = JSON.parse(value);
      onChange(JSON.stringify(parsed));
    } catch {
      // cannot minify invalid JSON
    }
  }, [language, value, onChange]);

  const handleCopy = useCallback(() => copyValue(value), [copyValue, value]);

  const handleScroll = useCallback(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    if (gutterRef.current) gutterRef.current.scrollTop = ta.scrollTop;
    if (highlightRef.current) {
      highlightRef.current.scrollTop = ta.scrollTop;
      highlightRef.current.scrollLeft = ta.scrollLeft;
    }
  }, []);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (readOnly) return;
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        handleFormat();
        return;
      }
      if (e.key === 'Tab') {
        e.preventDefault();
        const ta = e.currentTarget;
        const start = ta.selectionStart ?? 0;
        const end = ta.selectionEnd ?? start;
        const next = `${value.slice(0, start)}  ${value.slice(end)}`;
        onChange(next);
        window.requestAnimationFrame(() => {
          ta.selectionStart = start + 2;
          ta.selectionEnd = start + 2;
        });
      }
    },
    [readOnly, handleFormat, value, onChange],
  );

  const lines = useMemo(() => (value ? value.split('\n') : ['']), [value]);
  const isHighlighted = language !== 'plaintext' && lines.length <= 2000;

  return (
    <div className={cn('overflow-hidden rounded-lg border bg-card', className)}>
      <div className="flex flex-wrap items-center justify-between gap-2 border-b bg-muted/40 px-3 py-1.5">
        <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
          <Code2 className="h-3.5 w-3.5" />
          <span className="uppercase tracking-wider">{language}</span>
          <span className="text-[11px] tabular-nums opacity-70">{lines.length} ln</span>
          {syntaxErrors.length === 0 ? (
            <span className="inline-flex items-center gap-1 text-emerald-600 dark:text-emerald-400">
              <CheckCircle2 className="h-3.5 w-3.5" /> Valid
            </span>
          ) : (
            <span className="inline-flex items-center gap-1 text-red-600 dark:text-red-400">
              <AlertCircle className="h-3.5 w-3.5" /> {syntaxErrors.length} error(s)
            </span>
          )}
        </div>
        <div className="flex items-center gap-1">
          {!readOnly && language !== 'plaintext' && (
            <Button type="button" variant="ghost" size="sm" onClick={handleFormat} className="h-7 px-2 text-xs">
              <Sparkles className="h-3 w-3" /> Format
            </Button>
          )}
          {!readOnly && language === 'json' && (
            <Button type="button" variant="ghost" size="sm" onClick={handleMinify} className="h-7 px-2 text-xs">
              <Minimize2 className="h-3 w-3" /> Minify
            </Button>
          )}
          <Button type="button" variant="ghost" size="sm" onClick={handleCopy} className="h-7 px-2 text-xs">
            {copied ? <Check className="h-3 w-3 text-emerald-500" /> : <Copy className="h-3 w-3" />}
            {copied ? 'Copied' : 'Copy'}
          </Button>
        </div>
      </div>

      <div className="relative flex bg-background font-mono text-xs leading-5" style={{ height }}>
        <div
          ref={gutterRef}
          aria-hidden="true"
          className="w-10 shrink-0 select-none overflow-hidden border-r bg-muted/20 py-3 pr-2 text-right font-mono text-[11px] leading-5 text-muted-foreground/60 tabular-nums"
        >
          {lines.map((_, i) => (
            <div key={i}>{i + 1}</div>
          ))}
        </div>
        <div className="relative flex-1 overflow-hidden">
          {isHighlighted && (
            <pre
              ref={highlightRef}
              aria-hidden="true"
              className="pointer-events-none absolute inset-0 m-0 overflow-hidden whitespace-pre-wrap break-words p-3 font-mono text-xs leading-5 min-w-0"
            >
              {lines.map((line, i) => (
                <div key={i}>
                  {language === 'json' ? highlightJsonLine(line) : highlightYamlLine(line)}
                </div>
              ))}
            </pre>
          )}
          <textarea
            ref={textareaRef}
            aria-label={ariaLabel}
            value={value}
            readOnly={readOnly}
            onScroll={handleScroll}
            onKeyDown={handleKeyDown}
            onChange={(e) => onChange(e.target.value)}
            spellCheck={false}
            autoCapitalize="off"
            autoComplete="off"
            autoCorrect="off"
            className={cn(
              'relative z-10 h-full w-full resize-none bg-transparent p-3 font-mono text-xs leading-5 focus-visible:outline-none',
              isHighlighted
                ? 'text-transparent caret-foreground selection:bg-primary/25'
                : 'text-foreground',
            )}
          />
        </div>
      </div>

      {syntaxErrors.length > 0 && (
        <div role="alert" className="border-t border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-600 dark:text-red-400">
          {syntaxErrors.slice(0, 3).map((err, idx) => (
            <div key={idx}>{err}</div>
          ))}
        </div>
      )}
    </div>
  );
}
