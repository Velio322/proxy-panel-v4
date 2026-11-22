import { useId, useState } from 'react';
import { cn } from '@/lib/core-utils';

export const DEFAULT_PANEL_NAME = 'Keeper';

export const KEEPER_DEFAULT_FAVICON_DATA_URL =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 40 40'%3E%3Cdefs%3E%3ClinearGradient id='bg' x1='0' y1='0' x2='1' y2='1'%3E%3Cstop offset='0%25' stop-color='%2307382e'/%3E%3Cstop offset='100%25' stop-color='%23041b1e'/%3E%3C/linearGradient%3E%3ClinearGradient id='k' x1='0' y1='0' x2='1' y2='1'%3E%3Cstop offset='0%25' stop-color='%2334d399'/%3E%3Cstop offset='55%25' stop-color='%232dd4bf'/%3E%3Cstop offset='100%25' stop-color='%2306b6d4'/%3E%3C/linearGradient%3E%3C/defs%3E%3Crect x='1.5' y='1.5' width='37' height='37' rx='10' fill='url(%23bg)' stroke='%2334d399' stroke-opacity='0.48' stroke-width='1.5'/%3E%3Cpath d='M13 10.5V29.5M26.5 11L15 20.5L26.5 29.5' stroke='url(%23k)' stroke-width='3.2' stroke-linecap='round' stroke-linejoin='round' fill='none'/%3E%3Ccircle cx='13' cy='10.5' r='2.3' fill='%2334d399'/%3E%3Ccircle cx='13' cy='29.5' r='2.3' fill='%232dd4bf'/%3E%3Ccircle cx='26.5' cy='11' r='2.3' fill='%235eead4'/%3E%3Ccircle cx='26.5' cy='29.5' r='2.3' fill='%2306b6d4'/%3E%3Ccircle cx='15' cy='20.5' r='2.1' fill='%23ecfdf5'/%3E%3C/svg%3E";

export function KeeperLogoMark({ className }: { className?: string }) {
  const uid = useId().replace(/:/g, '');
  const bgId = `keeper-bg-${uid}`;
  const glowId = `keeper-glow-${uid}`;
  const routeId = `keeper-route-${uid}`;

  return (
    <svg
      viewBox="0 0 40 40"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={cn('h-full w-full shrink-0', className)}
      aria-hidden="true"
    >
      <defs>
        <linearGradient id={bgId} x1="4" y1="2" x2="36" y2="38" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#0a3b31" />
          <stop offset="55%" stopColor="#052122" />
          <stop offset="100%" stopColor="#04151b" />
        </linearGradient>
        <radialGradient id={glowId} cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" gradientTransform="translate(15 19) rotate(42) scale(20)">
          <stop offset="0%" stopColor="#34d399" stopOpacity="0.34" />
          <stop offset="100%" stopColor="#06b6d4" stopOpacity="0" />
        </radialGradient>
        <linearGradient id={routeId} x1="11" y1="9" x2="29" y2="31" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#6ee7b7" />
          <stop offset="50%" stopColor="#2dd4bf" />
          <stop offset="100%" stopColor="#06b6d4" />
        </linearGradient>
      </defs>

      <rect
        x="1"
        y="1"
        width="38"
        height="38"
        rx="10.5"
        fill={`url(#${bgId})`}
        stroke={`url(#${routeId})`}
        strokeOpacity="0.45"
        strokeWidth="1.25"
      />
      <rect x="2" y="2" width="36" height="36" rx="9.5" fill={`url(#${glowId})`} />

      {/* Subtle crystalline facet geometry */}
      <path
        d="M20 4.5L32.5 11.5V28.5L20 35.5L7.5 28.5V11.5L20 4.5Z"
        stroke="#34d399"
        strokeOpacity="0.16"
        strokeWidth="0.9"
      />

      {/* Geometric K network route */}
      <path
        d="M13.5 10.5V29.5M26.5 11L15.5 20.2L26.5 29.5"
        stroke={`url(#${routeId})`}
        strokeWidth="2.9"
        strokeLinecap="round"
        strokeLinejoin="round"
      />

      {/* Active route vertices */}
      <circle cx="13.5" cy="10.5" r="2.2" fill="#6ee7b7" />
      <circle cx="13.5" cy="29.5" r="2.2" fill="#2dd4bf" />
      <circle cx="26.5" cy="11" r="2.2" fill="#5eead4" />
      <circle cx="26.5" cy="29.5" r="2.2" fill="#22d3ee" />
      <circle cx="15.5" cy="20.2" r="2.4" fill="#ecfdf5" stroke="#10b981" strokeWidth="1.2" />
    </svg>
  );
}

interface BrandMarkProps {
  logoUrl?: string | null;
  panelName?: string;
  size?: 'xs' | 'sm' | 'md' | 'lg';
  className?: string;
}

const SIZE_CLASSES: Record<NonNullable<BrandMarkProps['size']>, string> = {
  xs: 'h-5 w-5 rounded-md',
  sm: 'h-7 w-7 rounded-lg',
  md: 'h-9 w-9 rounded-xl',
  lg: 'h-12 w-12 rounded-2xl',
};

export function BrandMark({ logoUrl, panelName = DEFAULT_PANEL_NAME, size = 'md', className }: BrandMarkProps) {
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  const showCustom = Boolean(logoUrl && failedUrl !== logoUrl);

  if (showCustom && logoUrl) {
    return (
      <span
        className={cn(
          'relative inline-flex shrink-0 items-center justify-center overflow-hidden border border-white/15 bg-emerald-950/35 shadow-[0_0_20px_-6px_rgba(16,185,129,0.45)]',
          SIZE_CLASSES[size],
          className,
        )}
      >
        <img
          src={logoUrl}
          alt={panelName}
          onError={() => setFailedUrl(logoUrl)}
          className="h-full w-full object-cover"
        />
      </span>
    );
  }

  return (
    <span
      className={cn(
        'relative inline-flex shrink-0 items-center justify-center overflow-hidden shadow-[0_0_22px_-6px_rgba(16,185,129,0.5)]',
        SIZE_CLASSES[size],
        className,
      )}
    >
      <KeeperLogoMark />
    </span>
  );
}

export function applyBrandingToDocument(
  panelName: string,
  logoUrl?: string | null,
  logoMime?: string | null,
) {
  if (typeof document === 'undefined') return;

  const cleanName = panelName.trim() || DEFAULT_PANEL_NAME;
  document.title = cleanName;

  let link = document.querySelector<HTMLLinkElement>('link[rel="icon"]')
    ?? document.querySelector<HTMLLinkElement>('link[rel="shortcut icon"]');

  if (!link) {
    link = document.createElement('link');
    link.rel = 'icon';
    document.head.appendChild(link);
  }

  if (logoUrl) {
    if (logoMime) {
      link.type = logoMime;
    } else if (logoUrl.startsWith('data:image/png')) {
      link.type = 'image/png';
    } else if (logoUrl.startsWith('data:image/webp')) {
      link.type = 'image/webp';
    } else if (logoUrl.startsWith('data:image/svg') || logoUrl.includes('.svg')) {
      link.type = 'image/svg+xml';
    } else {
      link.removeAttribute('type');
    }
    link.href = logoUrl;
  } else {
    link.type = 'image/svg+xml';
    link.href = KEEPER_DEFAULT_FAVICON_DATA_URL;
  }
}
