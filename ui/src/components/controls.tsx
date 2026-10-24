import { Fragment, type ButtonHTMLAttributes, type ComponentChildren } from "preact";
import { useId } from "preact/hooks";
import { keyLabel, modifierKind, orderedKeys } from "../lib/keys";

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger" | "inverse" | "inverse-ghost";

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "icon" | "size"> {
  variant?: ButtonVariant;
  size?: "md" | "sm";
  icon?: ComponentChildren;
  busy?: boolean;
  children?: ComponentChildren;
}

export function Button({ variant = "secondary", size = "md", icon, busy = false, children, class: className, disabled, type, ...rest }: ButtonProps) {
  return (
    <button
      {...rest}
      type={type ?? "button"}
      class={`button button-${variant} button-${size}${busy ? " is-busy" : ""}${className ? ` ${className}` : ""}`}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
    >
      {icon}
      {children !== undefined && <span>{children}</span>}
    </button>
  );
}

export function IconButton({ label, icon, class: className, ...rest }: Omit<ButtonHTMLAttributes<HTMLButtonElement>, "icon"> & { label: string; icon: ComponentChildren }) {
  return (
    <button {...rest} type="button" class={`icon-button${className ? ` ${className}` : ""}`} aria-label={label} title={label}>
      {icon}
    </button>
  );
}

export function Switch({ checked, onChange, label, disabled = false, describedBy }: { checked: boolean; onChange: (value: boolean) => void; label: string; disabled?: boolean; describedBy?: string }) {
  return (
    <button
      type="button"
      role="switch"
      class="switch"
      aria-checked={checked}
      aria-label={label}
      aria-describedby={describedBy}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <span class="switch-thumb" />
    </button>
  );
}

export interface SegmentOption<T extends string> {
  value: T;
  label: string;
  icon?: ComponentChildren;
}

/** A radio group drawn as segments; arrow keys move the selection, as Windows expects. */
export function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: SegmentOption<T>[]; onChange: (value: T) => void; label: string }) {
  const onKeyDown = (event: KeyboardEvent) => {
    const index = options.findIndex((option) => option.value === value);
    const delta = event.key === "ArrowRight" || event.key === "ArrowDown" ? 1 : event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 0;
    if (!delta) return;
    event.preventDefault();
    const next = options[(index + delta + options.length) % options.length];
    if (next) {
      onChange(next.value);
      const group = event.currentTarget as HTMLElement;
      requestAnimationFrame(() => group.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
    }
  };
  return (
    <div class="segmented" role="radiogroup" aria-label={label} onKeyDown={onKeyDown}>
      {options.map((option) => {
        const selected = option.value === value;
        return (
          <button type="button" role="radio" aria-checked={selected} tabIndex={selected ? 0 : -1} class="segment" onClick={() => onChange(option.value)} key={option.value}>
            {option.icon}
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

export function Select<T extends string>({ value, options, onChange, label, id, wide = false }: { value: T; options: { value: T; label: string }[]; onChange: (value: T) => void; label: string; id?: string; wide?: boolean }) {
  return (
    <span class={`select${wide ? " select-wide" : ""}`}>
      <select id={id} aria-label={label} value={value} onChange={(event) => onChange((event.currentTarget as HTMLSelectElement).value as T)}>
        {options.map((option) => (
          <option value={option.value} key={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <svg class="select-chevron" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
        <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </span>
  );
}

interface FieldProps {
  label: string;
  hint?: string;
  error?: string | null;
  children: (props: { id: string; describedBy: string | undefined; invalid: boolean }) => ComponentChildren;
}

/** Visible label, optional hint, and an inline error tied to the control. */
export function Field({ label, hint, error, children }: FieldProps) {
  const id = useId();
  const hintId = hint ? `${id}-hint` : undefined;
  const errorId = error ? `${id}-error` : undefined;
  const describedBy = [hintId, errorId].filter(Boolean).join(" ") || undefined;
  return (
    <div class={`field${error ? " has-error" : ""}`}>
      <label class="field-label" for={id}>
        {label}
      </label>
      {children({ id, describedBy, invalid: !!error })}
      {hint && !error && (
        <p class="field-hint" id={hintId}>
          {hint}
        </p>
      )}
      {error && (
        <p class="field-error" id={errorId} role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

export function Chip({ children, icon, tone = "neutral", title }: { children: ComponentChildren; icon?: ComponentChildren; tone?: "neutral" | "strong" | "ok" | "warn" | "err"; title?: string | undefined }) {
  return (
    <span class={`chip chip-${tone}`} title={title}>
      {icon}
      {children}
    </span>
  );
}

function WinGlyph() {
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
      <path fill="currentColor" d="M0 0h7.4v7.4H0zM8.6 0H16v7.4H8.6zM0 8.6h7.4V16H0zM8.6 8.6H16V16H8.6z" />
    </svg>
  );
}

/** A chord drawn as physical keys. `size="hero"` is the large keycap used on Home and setup. */
export function Keys({ keys, size = "sm", label }: { keys: readonly number[]; size?: "sm" | "md" | "hero"; label?: string }) {
  const ordered = orderedKeys(keys);
  const text = label ?? ordered.map(keyLabel).join(" + ");
  return (
    <span class={`keys keys-${size}`} role="img" aria-label={text}>
      {ordered.map((key, index) => (
        <Fragment key={key}>
          {index > 0 && size === "hero" && (
            <span class="keys-plus" aria-hidden="true">
              +
            </span>
          )}
          <kbd class={`key${key === 0x20 ? " key-space" : ""}`}>
            {modifierKind(key) === "win" && <WinGlyph />}
            {keyLabel(key)}
          </kbd>
        </Fragment>
      ))}
    </span>
  );
}

export function ProgressBar({ value, label }: { value: number | null; label: string }) {
  const percent = value === null ? null : Math.round(Math.min(1, Math.max(0, value)) * 100);
  return (
    <div class={`progress${percent === null ? " is-indeterminate" : ""}`} role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent ?? undefined}>
      <span class="progress-fill" style={percent === null ? undefined : { width: `${percent}%` }} />
    </div>
  );
}
