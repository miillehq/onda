import { UserRound } from "lucide-preact";
import type { Profile } from "../generated/core";

let cached: { base64: string; url: string } | null = null;

/** A blob URL for the stored PNG (the CSP allows blob: images, not data: URLs). One is kept. */
export function pictureUrl(base64: string | null | undefined): string | null {
  if (!base64) return null;
  if (cached?.base64 === base64) return cached.url;
  if (cached) URL.revokeObjectURL(cached.url);
  const bytes = Uint8Array.from(atob(base64), (character) => character.charCodeAt(0));
  cached = { base64, url: URL.createObjectURL(new Blob([bytes], { type: "image/png" })) };
  return cached.url;
}

/** First letters of up to two words, for the picture-less avatar. */
export function initials(name: string | null | undefined): string {
  return (name ?? "")
    .split(" ")
    .filter(Boolean)
    .slice(0, 2)
    .map((word) => [...word][0]?.toUpperCase() ?? "")
    .join("");
}

/** The profile picture, the name's initials, or a neutral person glyph when anonymous. */
export function Avatar({ profile, size = 56 }: { profile: Profile | null; size?: number }) {
  const url = pictureUrl(profile?.picture);
  const letters = initials(profile?.name);
  return (
    <span class="avatar" style={{ width: `${size}px`, height: `${size}px`, fontSize: `${Math.round(size * 0.38)}px` }} aria-hidden="true">
      {url ? <img src={url} alt="" width={size} height={size} /> : letters ? <span>{letters}</span> : <UserRound size={Math.round(size * 0.46)} />}
    </span>
  );
}
