import { useCallback, useEffect, useRef, useState } from 'react';
import {
  Check,
  Circle,
  RotateCcw,
  RotateCw,
  Square,
  X,
  ZoomIn,
  ZoomOut,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { cn } from '@/lib/core-utils';

export type CropMaskShape = 'circle' | 'squircle';

interface LogoCropDialogProps {
  open: boolean;
  sourceUrl: string | null;
  isRu: boolean;
  onClose: () => void;
  onApply: (croppedDataUrl: string) => void;
}

const CROP_BOX_SIZE = 260;
const WORKSPACE_SIZE = 320;
const EXPORT_SIZE = 512;
const MIN_ZOOM = 1;
const MAX_ZOOM = 4;

function drawRoundedRectPath(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
) {
  const radius = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + radius, y);
  ctx.arcTo(x + w, y, x + w, y + h, radius);
  ctx.arcTo(x + w, y + h, x, y + h, radius);
  ctx.arcTo(x, y + h, x, y, radius);
  ctx.arcTo(x, y, x + w, y, radius);
  ctx.closePath();
}

export function LogoCropDialog({
  open,
  sourceUrl,
  isRu,
  onClose,
  onApply,
}: LogoCropDialogProps) {
  const imgRef = useRef<HTMLImageElement | null>(null);
  const [imgSize, setImgSize] = useState<{ w: number; h: number } | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [maskShape, setMaskShape] = useState<CropMaskShape>('circle');
  const [zoom, setZoom] = useState<number>(1);
  const [rotation, setRotation] = useState<number>(0);
  const [pan, setPan] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const dragStartRef = useRef<{ pointerX: number; pointerY: number; panX: number; panY: number } | null>(
    null,
  );

  useEffect(() => {
    if (!open || !sourceUrl) {
      setImgSize(null);
      setLoadError(null);
      return;
    }

    let cancelled = false;
    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.onload = () => {
      if (cancelled) return;
      const w = img.naturalWidth || img.width || 256;
      const h = img.naturalHeight || img.height || 256;
      imgRef.current = img;
      setImgSize({ w, h });
      setZoom(1);
      setRotation(0);
      setPan({ x: 0, y: 0 });
      setLoadError(null);
    };
    img.onerror = () => {
      if (cancelled) return;
      setLoadError(
        isRu
          ? 'Не удалось декодировать изображение для обрезки'
          : 'Failed to decode image for cropping',
      );
    };
    img.src = sourceUrl;

    return () => {
      cancelled = true;
    };
  }, [open, sourceUrl, isRu]);

  const computeBaseScale = useCallback(
    (size: { w: number; h: number } | null, rot: number) => {
      if (!size) return 1;
      const rotW = rot % 180 === 0 ? size.w : size.h;
      const rotH = rot % 180 === 0 ? size.h : size.w;
      return Math.max(CROP_BOX_SIZE / Math.max(1, rotW), CROP_BOX_SIZE / Math.max(1, rotH));
    },
    [],
  );

  const clampPan = useCallback(
    (
      nextPan: { x: number; y: number },
      nextZoom: number,
      nextRot: number,
      size: { w: number; h: number } | null,
    ) => {
      if (!size) return { x: 0, y: 0 };
      const rotW = nextRot % 180 === 0 ? size.w : size.h;
      const rotH = nextRot % 180 === 0 ? size.h : size.w;
      const effScale = computeBaseScale(size, nextRot) * nextZoom;
      const dispW = rotW * effScale;
      const dispH = rotH * effScale;
      const maxX = Math.max(0, (dispW - CROP_BOX_SIZE) / 2);
      const maxY = Math.max(0, (dispH - CROP_BOX_SIZE) / 2);
      return {
        x: Math.max(-maxX, Math.min(maxX, nextPan.x)),
        y: Math.max(-maxY, Math.min(maxY, nextPan.y)),
      };
    },
    [computeBaseScale],
  );

  const handleZoomChange = (nextZoomRaw: number) => {
    const clampedZoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, Number(nextZoomRaw.toFixed(2))));
    setZoom(clampedZoom);
    setPan((prev) => clampPan(prev, clampedZoom, rotation, imgSize));
  };

  const handleRotate90 = () => {
    const nextRot = (rotation + 90) % 360;
    setRotation(nextRot);
    setPan((prev) => clampPan(prev, zoom, nextRot, imgSize));
  };

  const handleReset = () => {
    setZoom(1);
    setRotation(0);
    setPan({ x: 0, y: 0 });
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!imgSize) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    setIsDragging(true);
    dragStartRef.current = {
      pointerX: e.clientX,
      pointerY: e.clientY,
      panX: pan.x,
      panY: pan.y,
    };
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDragging || !dragStartRef.current) return;
    const dx = e.clientX - dragStartRef.current.pointerX;
    const dy = e.clientY - dragStartRef.current.pointerY;
    setPan(
      clampPan(
        {
          x: dragStartRef.current.panX + dx,
          y: dragStartRef.current.panY + dy,
        },
        zoom,
        rotation,
        imgSize,
      ),
    );
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (isDragging) {
      try {
        e.currentTarget.releasePointerCapture(e.pointerId);
      } catch {
        // ignore if not captured
      }
    }
    setIsDragging(false);
    dragStartRef.current = null;
  };

  const handleWheel = (e: React.WheelEvent<HTMLDivElement>) => {
    e.preventDefault();
    const delta = e.deltaY < 0 ? 0.12 : -0.12;
    handleZoomChange(zoom + delta);
  };

  const handleConfirmCrop = () => {
    const img = imgRef.current;
    if (!img || !imgSize) return;

    const canvas = document.createElement('canvas');
    canvas.width = EXPORT_SIZE;
    canvas.height = EXPORT_SIZE;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = 'high';
    ctx.clearRect(0, 0, EXPORT_SIZE, EXPORT_SIZE);

    ctx.save();
    if (maskShape === 'circle') {
      ctx.beginPath();
      ctx.arc(EXPORT_SIZE / 2, EXPORT_SIZE / 2, EXPORT_SIZE / 2, 0, Math.PI * 2);
      ctx.closePath();
      ctx.clip();
    } else {
      const exportRadius = Math.round(56 * (EXPORT_SIZE / CROP_BOX_SIZE));
      drawRoundedRectPath(ctx, 0, 0, EXPORT_SIZE, EXPORT_SIZE, exportRadius);
      ctx.clip();
    }

    const ratio = EXPORT_SIZE / CROP_BOX_SIZE;
    const baseScale = computeBaseScale(imgSize, rotation);
    const drawScale = baseScale * zoom * ratio;

    ctx.translate(EXPORT_SIZE / 2 + pan.x * ratio, EXPORT_SIZE / 2 + pan.y * ratio);
    ctx.rotate((rotation * Math.PI) / 180);
    ctx.scale(drawScale, drawScale);
    ctx.drawImage(img, -imgSize.w / 2, -imgSize.h / 2, imgSize.w, imgSize.h);
    ctx.restore();

    let dataUrl = canvas.toDataURL('image/webp', 0.92);
    if (!dataUrl.startsWith('data:image/webp') && !dataUrl.startsWith('data:image/png')) {
      dataUrl = canvas.toDataURL('image/png');
    }
    onApply(dataUrl);
  };

  const baseScale = computeBaseScale(imgSize, rotation);
  const effectiveScale = baseScale * zoom;
  const insetOffset = (WORKSPACE_SIZE - CROP_BOX_SIZE) / 2;

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent size="md" className="border-border/90 bg-zinc-950 text-zinc-100 p-5 shadow-2xl">
        <DialogHeader className="space-y-1">
          <div className="flex items-center justify-between gap-2">
            <DialogTitle className="text-base font-bold text-zinc-100">
              {isRu ? 'Кадрирование иконки' : 'Crop Panel Icon'}
            </DialogTitle>

            {/* Mask shape toggle: Circle vs Squircle */}
            <div className="inline-flex rounded-lg border border-white/10 bg-white/[0.04] p-0.5">
              <button
                type="button"
                onClick={() => setMaskShape('circle')}
                className={cn(
                  'inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium transition-colors',
                  maskShape === 'circle'
                    ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/35'
                    : 'text-zinc-400 hover:text-zinc-200',
                )}
              >
                <Circle className="h-3 w-3" />
                <span>{isRu ? 'Круг' : 'Circle'}</span>
              </button>
              <button
                type="button"
                onClick={() => setMaskShape('squircle')}
                className={cn(
                  'inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium transition-colors',
                  maskShape === 'squircle'
                    ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/35'
                    : 'text-zinc-400 hover:text-zinc-200',
                )}
              >
                <Square className="h-3 w-3" />
                <span>{isRu ? 'Квадрат' : 'Squircle'}</span>
              </button>
            </div>
          </div>
          <DialogDescription className="text-xs text-zinc-400">
            {isRu
              ? 'Перетащите изображение и настройте масштаб. Экспортируется в 256×256 px для сайдбара и favicon.'
              : 'Drag to reposition and adjust zoom. Exported at 256×256 px for sidebar and favicon.'}
          </DialogDescription>
        </DialogHeader>

        {/* Telegram-style dark interactive crop viewport */}
        <div className="flex flex-col items-center gap-4">
          <div
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onPointerCancel={handlePointerUp}
            onWheel={handleWheel}
            style={{ width: WORKSPACE_SIZE, height: WORKSPACE_SIZE }}
            className={cn(
              'relative select-none overflow-hidden rounded-2xl border border-white/10 bg-[#070a0f] shadow-inner touch-none',
              isDragging ? 'cursor-grabbing' : 'cursor-grab',
            )}
          >
            {/* Subtle checkerboard backdrop for transparent images */}
            <div
              aria-hidden="true"
              className="pointer-events-none absolute inset-0 opacity-15"
              style={{
                backgroundImage:
                  'linear-gradient(45deg, #1f2937 25%, transparent 25%), linear-gradient(-45deg, #1f2937 25%, transparent 25%), linear-gradient(45deg, transparent 75%, #1f2937 75%), linear-gradient(-45deg, transparent 75%, #1f2937 75%)',
                backgroundSize: '16px 16px',
                backgroundPosition: '0 0, 0 8px, 8px -8px, -8px 0',
              }}
            />

            {loadError ? (
              <div className="flex h-full items-center justify-center px-4 text-center text-xs text-red-400">
                {loadError}
              </div>
            ) : (
              sourceUrl &&
              imgSize && (
                <img
                  src={sourceUrl}
                  alt="Crop source"
                  draggable={false}
                  style={{
                    width: imgSize.w,
                    height: imgSize.h,
                    maxWidth: 'none',
                    maxHeight: 'none',
                    left: '50%',
                    top: '50%',
                    transform: `translate3d(calc(-50% + ${pan.x}px), calc(-50% + ${pan.y}px), 0) rotate(${rotation}deg) scale(${effectiveScale})`,
                    transformOrigin: 'center center',
                  }}
                  className="pointer-events-none absolute select-none will-change-transform"
                />
              )
            )}

            {/* Telegram-style dark vignette mask + square frame & circular/squircle guide */}
            <svg
              viewBox={`0 0 ${WORKSPACE_SIZE} ${WORKSPACE_SIZE}`}
              className="pointer-events-none absolute inset-0 h-full w-full"
              aria-hidden="true"
            >
              <defs>
                <mask id="telegram-crop-cutout">
                  <rect width={WORKSPACE_SIZE} height={WORKSPACE_SIZE} fill="white" />
                  {maskShape === 'circle' ? (
                    <circle
                      cx={WORKSPACE_SIZE / 2}
                      cy={WORKSPACE_SIZE / 2}
                      r={CROP_BOX_SIZE / 2}
                      fill="black"
                    />
                  ) : (
                    <rect
                      x={insetOffset}
                      y={insetOffset}
                      width={CROP_BOX_SIZE}
                      height={CROP_BOX_SIZE}
                      rx={56}
                      fill="black"
                    />
                  )}
                </mask>
              </defs>

              {/* Darkened outer vignette */}
              <rect
                width={WORKSPACE_SIZE}
                height={WORKSPACE_SIZE}
                fill="rgba(5, 8, 14, 0.72)"
                mask="url(#telegram-crop-cutout)"
              />

              {/* Outer square reference frame (Telegram-style square box around circle) */}
              <rect
                x={insetOffset}
                y={insetOffset}
                width={CROP_BOX_SIZE}
                height={CROP_BOX_SIZE}
                fill="none"
                stroke="rgba(255, 255, 255, 0.22)"
                strokeWidth="1"
              />

              {/* Subtle 3x3 rule-of-thirds grid visible while dragging */}
              {isDragging && (
                <g stroke="rgba(255, 255, 255, 0.18)" strokeWidth="0.75">
                  <line
                    x1={insetOffset + CROP_BOX_SIZE / 3}
                    y1={insetOffset}
                    x2={insetOffset + CROP_BOX_SIZE / 3}
                    y2={insetOffset + CROP_BOX_SIZE}
                  />
                  <line
                    x1={insetOffset + (CROP_BOX_SIZE * 2) / 3}
                    y1={insetOffset}
                    x2={insetOffset + (CROP_BOX_SIZE * 2) / 3}
                    y2={insetOffset + CROP_BOX_SIZE}
                  />
                  <line
                    x1={insetOffset}
                    y1={insetOffset + CROP_BOX_SIZE / 3}
                    x2={insetOffset + CROP_BOX_SIZE}
                    y2={insetOffset + CROP_BOX_SIZE / 3}
                  />
                  <line
                    x1={insetOffset}
                    y1={insetOffset + (CROP_BOX_SIZE * 2) / 3}
                    x2={insetOffset + CROP_BOX_SIZE}
                    y2={insetOffset + (CROP_BOX_SIZE * 2) / 3}
                  />
                </g>
              )}

              {/* Crisp active crop contour */}
              {maskShape === 'circle' ? (
                <circle
                  cx={WORKSPACE_SIZE / 2}
                  cy={WORKSPACE_SIZE / 2}
                  r={CROP_BOX_SIZE / 2}
                  fill="none"
                  stroke="#34d399"
                  strokeOpacity="0.85"
                  strokeWidth="1.75"
                />
              ) : (
                <rect
                  x={insetOffset}
                  y={insetOffset}
                  width={CROP_BOX_SIZE}
                  height={CROP_BOX_SIZE}
                  rx={56}
                  fill="none"
                  stroke="#34d399"
                  strokeOpacity="0.85"
                  strokeWidth="1.75"
                />
              )}
            </svg>
          </div>

          {/* Zoom slider bar */}
          <div className="flex w-full items-center gap-2.5 px-2">
            <button
              type="button"
              onClick={() => handleZoomChange(zoom - 0.2)}
              disabled={zoom <= MIN_ZOOM}
              aria-label="Zoom out"
              className="rounded-md p-1.5 text-zinc-400 transition-colors hover:bg-white/[0.06] hover:text-zinc-100 disabled:opacity-35"
            >
              <ZoomOut className="h-4 w-4" />
            </button>
            <input
              type="range"
              min={MIN_ZOOM}
              max={MAX_ZOOM}
              step={0.02}
              value={zoom}
              onChange={(e) => handleZoomChange(Number(e.target.value))}
              aria-label={isRu ? 'Масштаб' : 'Zoom'}
              className="h-1.5 flex-1 cursor-pointer accent-emerald-400"
            />
            <button
              type="button"
              onClick={() => handleZoomChange(zoom + 0.2)}
              disabled={zoom >= MAX_ZOOM}
              aria-label="Zoom in"
              className="rounded-md p-1.5 text-zinc-400 transition-colors hover:bg-white/[0.06] hover:text-zinc-100 disabled:opacity-35"
            >
              <ZoomIn className="h-4 w-4" />
            </button>
            <span className="w-11 text-right font-mono text-xs text-zinc-400 tabular-nums">
              {zoom.toFixed(1)}×
            </span>
          </div>

          {/* Bottom Telegram-style action bar: Rotate 90, Reset, Cancel, Confirm Checkmark */}
          <div className="flex w-full items-center justify-between border-t border-white/10 pt-3">
            <div className="flex items-center gap-1.5">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handleRotate90}
                className="h-8 gap-1.5 border-white/10 bg-white/[0.04] text-xs text-zinc-200 hover:bg-white/[0.09]"
                title={isRu ? 'Повернуть на 90°' : 'Rotate 90°'}
              >
                <RotateCw className="h-3.5 w-3.5" />
                <span>90°</span>
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={handleReset}
                className="h-8 gap-1 text-xs text-zinc-400 hover:bg-white/[0.06] hover:text-zinc-200"
                title={isRu ? 'Сбросить кадрирование' : 'Reset framing'}
              >
                <RotateCcw className="h-3.5 w-3.5" />
                <span>{isRu ? 'Сброс' : 'Reset'}</span>
              </Button>
            </div>

            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={onClose}
                className="h-9 gap-1 text-xs text-zinc-400 hover:bg-white/[0.06] hover:text-zinc-200"
              >
                <X className="h-4 w-4" />
                <span>{isRu ? 'Отмена' : 'Cancel'}</span>
              </Button>
              <Button
                type="button"
                size="sm"
                disabled={!imgSize || Boolean(loadError)}
                onClick={handleConfirmCrop}
                className="h-9 gap-1.5 rounded-full bg-emerald-500 px-4 text-xs font-semibold text-zinc-950 shadow-[0_0_20px_-4px_#10b981] hover:bg-emerald-400"
              >
                <Check className="h-4 w-4 stroke-[2.5]" />
                <span>{isRu ? 'Применить' : 'Apply'}</span>
              </Button>
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
