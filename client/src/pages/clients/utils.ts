import { generateStrongPassword } from '@/lib/core-utils';
import { usePublicConfigStore } from '@/lib/store';

// CSPRNG password generation (zero Math.random)
export function generateRandomPassword(length = 16): string {
  return generateStrongPassword(length);
}

// B1: SUB_URL from /api/v1/public-config, falling back to window.location.origin
export function getSubBaseUrl(overrideBase?: string): string {
  const configured = overrideBase ?? usePublicConfigStore.getState().config?.subUrl;
  const fallbackOrigin = typeof window !== 'undefined' ? window.location.origin : '';
  const raw = configured && configured.trim() ? configured.trim() : fallbackOrigin;
  return raw.replace(/\/+$/, '');
}

export function buildSubUrl(subToken: string, flag?: string, overrideBase?: string): string {
  const base = getSubBaseUrl(overrideBase);
  const url = `${base}/sub/${encodeURIComponent(subToken)}`;
  return flag ? `${url}?flag=${encodeURIComponent(flag)}` : url;
}

export function buildApiSubJsonUrl(subToken: string): string {
  return `/api/v1/client/${encodeURIComponent(subToken)}/sub?flag=json`;
}

export interface ParsedCsvClientRow {
  username: string;
  email?: string;
  trafficLimitGb: number;
  expireDays?: number;
  protocols: string[];
  note?: string;
  planId?: string;
}

export function parseCsvClients(csvText: string): { rows: ParsedCsvClientRow[]; errors: string[] } {
  const lines = csvText
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter(Boolean);
  if (lines.length === 0) {
    return { rows: [], errors: ['CSV is empty'] };
  }

  const rows: ParsedCsvClientRow[] = [];
  const errors: string[] = [];
  const startIdx = lines[0].toLowerCase().startsWith('username') ? 1 : 0;

  for (let i = startIdx; i < lines.length; i++) {
    const cols = lines[i].split(',').map((c) => c.trim());
    const username = cols[0];
    if (!username) {
      errors.push(`Line ${i + 1}: missing username`);
      continue;
    }
    const email = cols[1] || undefined;
    const trafficLimitGb = cols[2] ? Number(cols[2]) : 0;
    if (!Number.isFinite(trafficLimitGb) || trafficLimitGb < 0) {
      errors.push(`Line ${i + 1}: invalid trafficLimitGb "${cols[2]}"`);
      continue;
    }
    const expireDays = cols[3] ? Number(cols[3]) : undefined;
    if (expireDays !== undefined && (!Number.isFinite(expireDays) || expireDays < 0)) {
      errors.push(`Line ${i + 1}: invalid expireDays "${cols[3]}"`);
      continue;
    }
    const protocols = cols[4]
      ? cols[4]
          .split(/[;|]/)
          .map((p) => p.trim().toUpperCase())
          .filter(Boolean)
      : ['VLESS'];
    rows.push({
      username,
      email,
      trafficLimitGb,
      expireDays,
      protocols,
      note: cols[5] || undefined,
      planId: cols[6] || undefined,
    });
  }

  return { rows, errors };
}
