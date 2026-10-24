/** Windows virtual-key codes for shortcut display and recording. Policy (conflicts, reserved keys)
 *  stays native; this only translates between keyboard events and the stored chord. */

export const VK = {
  shift: 0x10,
  control: 0x11,
  alt: 0x12,
  leftShift: 0xa0,
  rightShift: 0xa1,
  leftControl: 0xa2,
  rightControl: 0xa3,
  leftAlt: 0xa4,
  rightAlt: 0xa5,
  leftWin: 0x5b,
  rightWin: 0x5c,
  escape: 0x1b,
} as const;

export type ModifierKind = "ctrl" | "shift" | "alt" | "win";

export function modifierKind(key: number): ModifierKind | null {
  if (key === VK.control || key === VK.leftControl || key === VK.rightControl) return "ctrl";
  if (key === VK.shift || key === VK.leftShift || key === VK.rightShift) return "shift";
  if (key === VK.alt || key === VK.leftAlt || key === VK.rightAlt) return "alt";
  if (key === VK.leftWin || key === VK.rightWin) return "win";
  return null;
}

const MODIFIER_ORDER: ModifierKind[] = ["ctrl", "shift", "alt", "win"];
const MODIFIER_LABEL: Record<ModifierKind, string> = { ctrl: "Ctrl", shift: "Shift", alt: "Alt", win: "Win" };
const MODIFIER_CODE: Record<ModifierKind, number> = { ctrl: VK.leftControl, shift: VK.leftShift, alt: VK.leftAlt, win: VK.leftWin };

const NAMED: Record<number, string> = {
  0x08: "Backspace", 0x09: "Tab", 0x0d: "Enter", 0x13: "Pause", 0x1b: "Esc", 0x14: "Caps Lock", 0x20: "Space",
  0x21: "Page Up", 0x22: "Page Down", 0x23: "End", 0x24: "Home", 0x25: "←", 0x26: "↑", 0x27: "→", 0x28: "↓",
  0x2c: "Print Screen", 0x2d: "Insert", 0x2e: "Delete", 0x5d: "Menu", 0x91: "Scroll Lock",
  0x6a: "Num *", 0x6b: "Num +", 0x6d: "Num -", 0x6e: "Num .", 0x6f: "Num /",
  0xba: ";", 0xbb: "=", 0xbc: ",", 0xbd: "-", 0xbe: ".", 0xbf: "/", 0xc0: "`", 0xdb: "[", 0xdc: "\\", 0xdd: "]", 0xde: "'",
};

export function keyLabel(key: number): string {
  const kind = modifierKind(key);
  if (kind) return MODIFIER_LABEL[kind];
  if ((key >= 0x30 && key <= 0x39) || (key >= 0x41 && key <= 0x5a)) return String.fromCharCode(key);
  if (key >= 0x60 && key <= 0x69) return `Num ${key - 0x60}`;
  if (key >= 0x70 && key <= 0x87) return `F${key - 0x6f}`;
  return NAMED[key] ?? `Key ${key}`;
}

/** Display order: Ctrl, Shift, Alt, Win, then the primary key. */
export function orderedKeys(keys: readonly number[]): number[] {
  const modifiers = MODIFIER_ORDER.filter((kind) => keys.some((key) => modifierKind(key) === kind)).map((kind) => MODIFIER_CODE[kind]);
  return [...modifiers, ...keys.filter((key) => modifierKind(key) === null)];
}

export function shortcutText(keys: readonly number[]): string {
  return orderedKeys(keys).map(keyLabel).join(" + ");
}

const CODE_TO_VK: Record<string, number> = {
  ControlLeft: VK.leftControl, ControlRight: VK.rightControl, ShiftLeft: VK.leftShift, ShiftRight: VK.rightShift,
  AltLeft: VK.leftAlt, AltRight: VK.rightAlt, MetaLeft: VK.leftWin, MetaRight: VK.rightWin, OSLeft: VK.leftWin, OSRight: VK.rightWin,
  Space: 0x20, Enter: 0x0d, Tab: 0x09, Backspace: 0x08, Insert: 0x2d, Delete: 0x2e, Home: 0x24, End: 0x23,
  PageUp: 0x21, PageDown: 0x22, ArrowLeft: 0x25, ArrowUp: 0x26, ArrowRight: 0x27, ArrowDown: 0x28,
  Pause: 0x13, CapsLock: 0x14, ScrollLock: 0x91, PrintScreen: 0x2c, ContextMenu: 0x5d, Escape: VK.escape,
  NumpadMultiply: 0x6a, NumpadAdd: 0x6b, NumpadSubtract: 0x6d, NumpadDecimal: 0x6e, NumpadDivide: 0x6f,
  Semicolon: 0xba, Equal: 0xbb, Comma: 0xbc, Minus: 0xbd, Period: 0xbe, Slash: 0xbf, Backquote: 0xc0,
  BracketLeft: 0xdb, Backslash: 0xdc, BracketRight: 0xdd, Quote: 0xde,
};

export function virtualKey(event: KeyboardEvent): number | null {
  const known = CODE_TO_VK[event.code];
  if (known !== undefined) return known;
  const letter = /^Key([A-Z])$/.exec(event.code);
  if (letter?.[1]) return letter[1].charCodeAt(0);
  const digit = /^Digit([0-9])$/.exec(event.code);
  if (digit?.[1]) return digit[1].charCodeAt(0);
  const numpad = /^Numpad([0-9])$/.exec(event.code);
  if (numpad?.[1]) return 0x60 + Number(numpad[1]);
  const fn = /^F([0-9]{1,2})$/.exec(event.code);
  if (fn?.[1]) {
    const index = Number(fn[1]);
    if (index >= 1 && index <= 24) return 0x6f + index;
  }
  return null;
}

/** Normalise a recorded set into the stored chord: left-hand modifiers, one primary key. */
export function chordFrom(keys: Iterable<number>): { keys: number[]; problem: string | null } {
  const kinds = new Set<ModifierKind>();
  const primaries: number[] = [];
  for (const key of keys) {
    const kind = modifierKind(key);
    if (kind) kinds.add(kind);
    else if (!primaries.includes(key)) primaries.push(key);
  }
  const modifiers = MODIFIER_ORDER.filter((kind) => kinds.has(kind)).map((kind) => MODIFIER_CODE[kind]);
  if (primaries.length === 0) return { keys: modifiers, problem: "Add one key that isn't Ctrl, Shift, Alt or Win." };
  if (primaries.length > 1) return { keys: [...modifiers, ...primaries], problem: "Use only one key besides Ctrl, Shift, Alt and Win." };
  if (primaries[0] === VK.escape) return { keys: modifiers, problem: "Esc is reserved for cancelling." };
  const chord = [...modifiers, primaries[0] as number];
  if (chord.length > 4) return { keys: chord, problem: "Use at most four keys." };
  return { keys: chord, problem: null };
}

export function sameChord(left: readonly number[], right: readonly number[]): boolean {
  const a = orderedKeys(left);
  const b = orderedKeys(right);
  return a.length === b.length && a.every((key, index) => key === b[index]);
}
