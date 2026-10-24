import type { ReadonlySignal } from "@preact/signals";
import { useEffect, useId, useRef } from "preact/hooks";
import { Smoother, wavePath } from "../lib/level";

export type WaveMode = "live" | "settle" | "still";

interface WaveProps {
  /** 0–1 input level; only read in `live` mode. */
  level?: ReadonlySignal<number>;
  mode: WaveMode;
  width?: number;
  height?: number;
  /** Fixed amplitude for decorative, non-live use (0–1). */
  idle?: number;
  /** Paint with the brand's glyph-on-graphite gradient instead of currentColor. */
  glyph?: boolean;
  class?: string;
}

const VIEW_W = 88;
const VIEW_H = 32;
const MID = 16;

/**
 * The Onda wave passing behind the cursor. In `live` mode it follows the microphone; in
 * `settle` it flattens into the cursor while words are written out. Animation runs only
 * while mounted in an animated mode and honours reduced motion.
 */
export function Wave({ level, mode, width = 88, height = 32, idle = 0.35, glyph = false, class: className }: WaveProps) {
  const gradientId = useId();
  const paint = glyph ? `url(#${gradientId})` : "currentColor";
  const left = useRef<SVGPathElement>(null);
  const right = useRef<SVGPathElement>(null);
  const pillar = useRef<SVGRectElement>(null);

  useEffect(() => {
    const draw = (amplitude: number, phase: number) => {
      left.current?.setAttribute("d", wavePath(4, 36.6, MID, amplitude, phase, true));
      right.current?.setAttribute("d", wavePath(51.4, 84, MID, amplitude, phase + Math.PI, false));
    };
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (mode === "still" || reduced) {
      draw(mode === "settle" ? 1.2 : 2.5 + idle * 8.5, 0.6);
      if (pillar.current) pillar.current.style.opacity = "1";
      return;
    }
    const smoother = new Smoother();
    let frame = 0;
    let phase = 0;
    let last = performance.now();
    const tick = (now: number) => {
      const elapsed = Math.min(64, now - last);
      last = now;
      if (mode === "live") {
        const value = smoother.next(level?.value ?? 0);
        phase += elapsed * (0.004 + value * 0.012);
        draw(1.2 + value * 10, phase);
      } else {
        phase += elapsed * 0.006;
        draw(1.1 + 0.6 * Math.sin(now / 180), phase);
        if (pillar.current) pillar.current.style.opacity = (0.62 + 0.38 * Math.sin(now / 160)).toFixed(2);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [mode, level, idle]);

  return (
    <svg class={className} viewBox={`0 0 ${VIEW_W} ${VIEW_H}`} width={width} height={height} aria-hidden="true" overflow="visible">
      {glyph && (
        <defs>
          <linearGradient id={gradientId} gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2={VIEW_H}>
            <stop offset="0" stop-color="#ffffff" />
            <stop offset="1" stop-color="#cbcbcb" />
          </linearGradient>
        </defs>
      )}
      <path ref={left} fill="none" stroke={paint} stroke-width="4.6" stroke-linecap="round" />
      <path ref={right} fill="none" stroke={paint} stroke-width="4.6" stroke-linecap="round" />
      <rect ref={pillar} x="41.7" y="2" width="4.6" height="28" rx="2.3" fill={paint} />
    </svg>
  );
}
