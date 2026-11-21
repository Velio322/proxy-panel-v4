import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';
import type { Language, Protocol } from '@/types';

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export const PROTOCOLS: readonly Protocol[] = [
  'VLESS',
  'VMESS',
  'TROJAN',
  'SHADOWSOCKS',
  'HYSTERIA2',
  'NAIVEPROXY',
  'MIERU',
  'TUIC',
  'ANYTLS',
  'SHADOWTLS',
] as const;

export const PROTOCOL_LABELS: Record<Protocol, string> = {
  VLESS: 'VLESS',
  VMESS: 'VMess',
  TROJAN: 'Trojan',
  SHADOWSOCKS: 'Shadowsocks',
  HYSTERIA2: 'Hysteria2',
  NAIVEPROXY: 'NaiveProxy',
  MIERU: 'Mieru',
  TUIC: 'TUIC',
  ANYTLS: 'AnyTLS',
  SHADOWTLS: 'ShadowTLS',
};

export const PROTOCOL_COLORS: Record<Protocol, string> = {
  VLESS: 'bg-sky-500/15 text-sky-400 border-sky-500/30',
  VMESS: 'bg-indigo-500/15 text-indigo-400 border-indigo-500/30',
  TROJAN: 'bg-amber-500/15 text-amber-400 border-amber-500/30',
  SHADOWSOCKS: 'bg-violet-500/15 text-violet-400 border-violet-500/30',
  HYSTERIA2: 'bg-emerald-500/15 text-emerald-400 border-emerald-500/30',
  NAIVEPROXY: 'bg-cyan-500/15 text-cyan-400 border-cyan-500/30',
  MIERU: 'bg-rose-500/15 text-rose-400 border-rose-500/30',
  TUIC: 'bg-teal-500/15 text-teal-400 border-teal-500/30',
  ANYTLS: 'bg-fuchsia-500/15 text-fuchsia-400 border-fuchsia-500/30',
  SHADOWTLS: 'bg-orange-500/15 text-orange-400 border-orange-500/30',
};

export type ProxyCore = 'xray' | 'singbox' | 'naive' | 'mieru';

export function getCoreForProtocol(protocol: Protocol): ProxyCore {
  switch (protocol) {
    case 'VLESS':
    case 'VMESS':
    case 'TROJAN':
    case 'SHADOWSOCKS':
      return 'xray';
    case 'HYSTERIA2':
    case 'TUIC':
    case 'ANYTLS':
    case 'SHADOWTLS':
      return 'singbox';
    case 'NAIVEPROXY':
      return 'naive';
    case 'MIERU':
      return 'mieru';
  }
}

export function isMeteredProtocol(_protocol: Protocol): boolean {
  // All 4 cores (Xray StatsService, Sing-box HY2/TUIC/AnyTLS, Mieru mita metrics, Naive Caddy log) support per-user metering
  return true;
}

// CSPRNG Generators (ZERO Math.random)

const DEFAULT_PASSWORD_ALPHABET =
  'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789';

export function randomBytes(length: number): Uint8Array {
  const buf = new Uint8Array(length);
  crypto.getRandomValues(buf);
  return buf;
}

export function randomHex(byteLength: number): string {
  return Array.from(randomBytes(byteLength), (b) => b.toString(16).padStart(2, '0')).join('');
}

export function randomString(length: number, alphabet = DEFAULT_PASSWORD_ALPHABET): string {
  if (length <= 0 || alphabet.length === 0) return '';
  const maxValid = Math.floor(256 / alphabet.length) * alphabet.length;
  let out = '';
  while (out.length < length) {
    const chunk = randomBytes(Math.max(16, (length - out.length) * 2));
    for (let i = 0; i < chunk.length && out.length < length; i++) {
      if (chunk[i] < maxValid) {
        out += alphabet[chunk[i] % alphabet.length];
      }
    }
  }
  return out;
}

export function generateStrongPassword(length = 16): string {
  return randomString(length, DEFAULT_PASSWORD_ALPHABET);
}

export function randomPassword(length = 16): string {
  return generateStrongPassword(length);
}

export function generateNodeSecret(byteLength = 24): string {
  return randomHex(byteLength);
}

export function generateShortId(byteLength = 8): string {
  return randomHex(byteLength); // 16 hex chars
}

function bytesToStandardBase64(bytes: Uint8Array): string {
  let bin = '';
  for (let i = 0; i < bytes.length; i++) {
    bin += String.fromCharCode(bytes[i]);
  }
  return btoa(bin);
}

export function generateSs2022Key(method: string): string {
  const byteLen = method.includes('128') ? 16 : 32;
  return bytesToStandardBase64(randomBytes(byteLen));
}

// RFC 7748 X25519 Base64url Reality Keypair

const P25519 = (1n << 255n) - 19n;
const A24 = 121665n;

function modP(a: bigint): bigint {
  const r = a % P25519;
  return r >= 0n ? r : r + P25519;
}

function powP(base: bigint, exp: bigint): bigint {
  let res = 1n;
  let b = modP(base);
  let e = exp;
  while (e > 0n) {
    if (e & 1n) res = modP(res * b);
    b = modP(b * b);
    e >>= 1n;
  }
  return res;
}

function invP(a: bigint): bigint {
  return powP(a, P25519 - 2n);
}

function decodeLittleEndian25519(u: Uint8Array): bigint {
  let k = 0n;
  for (let i = 0; i < 32; i++) {
    const byte = i === 31 ? u[i] & 0x7f : u[i];
    k |= BigInt(byte) << BigInt(8 * i);
  }
  return k;
}

function decodeScalar25519(kBytes: Uint8Array): bigint {
  const clamped = new Uint8Array(kBytes);
  clamped[0] &= 248;
  clamped[31] &= 127;
  clamped[31] |= 64;
  let k = 0n;
  for (let i = 0; i < 32; i++) {
    k |= BigInt(clamped[i]) << BigInt(8 * i);
  }
  return k;
}

function encodeLittleEndian25519(n: bigint): Uint8Array {
  const out = new Uint8Array(32);
  let v = modP(n);
  for (let i = 0; i < 32; i++) {
    out[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return out;
}

export function x25519ScalarMult(scalarBytes: Uint8Array, uBytes: Uint8Array): Uint8Array {
  if (scalarBytes.length !== 32 || uBytes.length !== 32) {
    throw new Error('X25519 inputs must be 32 bytes');
  }
  const k = decodeScalar25519(scalarBytes);
  const u = decodeLittleEndian25519(uBytes);

  let x1 = u;
  let x2 = 1n;
  let z2 = 0n;
  let x3 = u;
  let z3 = 1n;
  let swap = 0n;

  for (let t = 254; t >= 0; t--) {
    const kT = (k >> BigInt(t)) & 1n;
    swap ^= kT;
    if (swap === 1n) {
      [x2, x3] = [x3, x2];
      [z2, z3] = [z3, z2];
    }
    swap = kT;

    const a = modP(x2 + z2);
    const aa = modP(a * a);
    const b = modP(x2 - z2);
    const bb = modP(b * b);
    const e = modP(aa - bb);
    const c = modP(x3 + z3);
    const d = modP(x3 - z3);
    const da = modP(d * a);
    const cb = modP(c * b);

    const sumDaCb = modP(da + cb);
    const diffDaCb = modP(da - cb);

    x3 = modP(sumDaCb * sumDaCb);
    z3 = modP(x1 * modP(diffDaCb * diffDaCb));
    x2 = modP(aa * bb);
    z2 = modP(e * modP(aa + modP(A24 * e)));
  }

  if (swap === 1n) {
    [x2, x3] = [x3, x2];
    [z2, z3] = [z3, z2];
  }

  const res = modP(x2 * invP(z2));
  return encodeLittleEndian25519(res);
}

export function x25519PublicFromPrivate(privBytes: Uint8Array): Uint8Array {
  const basepoint = new Uint8Array(32);
  basepoint[0] = 9;
  return x25519ScalarMult(privBytes, basepoint);
}

export function toBase64Url(bytes: Uint8Array): string {
  return bytesToStandardBase64(bytes)
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/g, '');
}

export function fromBase64Url(str: string): Uint8Array {
  const padded = str.replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((str.length + 3) % 4);
  const bin = atob(padded);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) {
    out[i] = bin.charCodeAt(i);
  }
  return out;
}

export function generateRealityKeypair(): { privateKey: string; publicKey: string } {
  const priv = randomBytes(32);
  priv[0] &= 248;
  priv[31] &= 127;
  priv[31] |= 64;
  const pub = x25519PublicFromPrivate(priv);
  return {
    privateKey: toBase64Url(priv),
    publicKey: toBase64Url(pub),
  };
}

// --- Formatting Utilities (Timezone-aware Date & Localized Compact Bytes)

let activeTimezone: string | undefined;
let activeLocale: Language = 'en';

export function setFormattingDefaults(opts: { timezone?: string; language?: Language }) {
  if (opts.timezone !== undefined) {
    activeTimezone = opts.timezone.trim() || undefined;
  }
  if (opts.language !== undefined) {
    activeLocale = opts.language;
  }
}

export function formatDate(
  value: string | number | Date | null | undefined,
  options?: Intl.DateTimeFormatOptions,
  timezone?: string,
  locale?: string,
): string {
  if (value === null || value === undefined || value === '') return '—';
  const d = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(d.getTime())) return '—';
  const tz = (timezone ?? activeTimezone)?.trim() || undefined;
  const loc = locale ?? (activeLocale === 'ru' ? 'ru-RU' : 'en-US');
  const baseOpts: Intl.DateTimeFormatOptions = {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    ...options,
  };
  try {
    return new Intl.DateTimeFormat(loc, tz ? { ...baseOpts, timeZone: tz } : baseOpts).format(d);
  } catch {
    return new Intl.DateTimeFormat(loc, baseOpts).format(d);
  }
}

export function formatDateTime(
  value: string | number | Date | null | undefined,
  timezone?: string,
  locale?: string,
): string {
  return formatDate(
    value,
    {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    },
    timezone,
    locale,
  );
}

export interface FormatBytesOptions {
  compact?: boolean;
  locale?: Language;
  decimals?: number;
}

export function formatBytes(
  bytesInput: number | string | null | undefined,
  compactOrOpts: boolean | FormatBytesOptions = false,
): string {
  const opts: FormatBytesOptions =
    typeof compactOrOpts === 'boolean' ? { compact: compactOrOpts } : compactOrOpts;
  const compact = opts.compact ?? false;
  const locale = opts.locale ?? activeLocale;
  const decimals = opts.decimals ?? (compact ? 1 : 2);

  const bytes = typeof bytesInput === 'string' ? Number(bytesInput) : (bytesInput ?? 0);
  if (!Number.isFinite(bytes) || bytes <= 0) {
    if (compact) return locale === 'ru' ? '0Б' : '0B';
    return locale === 'ru' ? '0 Б' : '0 B';
  }

  const units =
    locale === 'ru'
      ? compact
        ? ['Б', 'К', 'М', 'Г', 'Т', 'П']
        : ['Б', 'КБ', 'МБ', 'ГБ', 'ТБ', 'ПБ']
      : compact
        ? ['B', 'K', 'M', 'G', 'T', 'P']
        : ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];

  const k = 1024;
  const idx = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), units.length - 1);
  const val = bytes / Math.pow(k, idx);
  const formatted = idx === 0 ? Math.round(val).toString() : val.toFixed(decimals).replace(/\.0+$/, '');
  return compact ? `${formatted}${units[idx]}` : `${formatted} ${units[idx]}`;
}

export function formatBps(bps: number | null | undefined, locale?: Language): string {
  if (bps === null || bps === undefined || !Number.isFinite(bps) || bps < 0) {
    return '—';
  }
  return `${formatBytes(bps, { compact: false, locale })}/s`;
}

export function parsePortRange(spec: string): { valid: boolean; count: number; ports: number[]; error?: string } {
  const trimmed = spec.trim();
  if (!trimmed) {
    return { valid: true, count: 0, ports: [] };
  }
  const seen = new Set<number>();
  const parts = trimmed.split(',');
  for (const rawPart of parts) {
    const part = rawPart.trim();
    if (!part) {
      return { valid: false, count: 0, ports: [], error: 'Empty port segment' };
    }
    if (part.includes('-')) {
      const bounds = part.split('-');
      if (bounds.length !== 2 || !/^\d+$/.test(bounds[0].trim()) || !/^\d+$/.test(bounds[1].trim())) {
        return { valid: false, count: 0, ports: [], error: `Invalid port range "${part}"` };
      }
      const start = Number(bounds[0].trim());
      const end = Number(bounds[1].trim());
      if (start < 1 || end > 65535 || start > end) {
        return { valid: false, count: 0, ports: [], error: `Out-of-bounds range "${part}"` };
      }
      if (end - start + 1 > 64) {
        return { valid: false, count: end - start + 1, ports: [], error: 'Port hopping supports at most 64 ports' };
      }
      for (let p = start; p <= end; p++) {
        seen.add(p);
        if (seen.size > 64) {
          return { valid: false, count: seen.size, ports: [], error: 'Port hopping supports at most 64 ports' };
        }
      }
    } else {
      if (!/^\d+$/.test(part)) {
        return { valid: false, count: 0, ports: [], error: `Invalid port "${part}"` };
      }
      const p = Number(part);
      if (p < 1 || p > 65535) {
        return { valid: false, count: 0, ports: [], error: `Port ${p} must be 1..65535` };
      }
      seen.add(p);
      if (seen.size > 64) {
        return { valid: false, count: seen.size, ports: [], error: 'Port hopping supports at most 64 ports' };
      }
    }
  }
  const ports = Array.from(seen).sort((a, b) => a - b);
  return { valid: true, count: ports.length, ports };
}
