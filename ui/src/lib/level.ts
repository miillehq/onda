/** Map short-window RMS to a 0–1 display level on a speech-friendly dB scale. */
export function levelFromRms(rms: number): number {
  if (!(rms > 0)) return 0;
  const db = 20 * Math.log10(rms);
  return Math.min(1, Math.max(0, (db + 55) / 40));
}

/** Fast attack, slow release, so the wave reacts to syllables but never flickers. */
export class Smoother {
  value = 0;
  next(target: number, attack = 0.5, release = 0.12): number {
    const rate = target > this.value ? attack : release;
    this.value += (target - this.value) * rate;
    if (this.value < 0.001) this.value = 0;
    return this.value;
  }
}

/** A wave along y = mid between x0 and x1, tapered at its outer end like a voice burst. */
export function wavePath(x0: number, x1: number, mid: number, amplitude: number, phase: number, outerAtStart: boolean, cycles = 1.1): string {
  const steps = 28;
  let d = "";
  for (let index = 0; index <= steps; index += 1) {
    const u = index / steps;
    const x = x0 + (x1 - x0) * u;
    const fromOuter = outerAtStart ? u : 1 - u;
    const taper = Math.min(1, fromOuter * 2.2 + 0.12);
    const y = mid + amplitude * taper * Math.sin(u * Math.PI * 2 * cycles + phase);
    d += `${index === 0 ? "M" : "L"}${x.toFixed(2)} ${y.toFixed(2)}`;
  }
  return d;
}
