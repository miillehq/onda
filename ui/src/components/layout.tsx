import type { ComponentChildren } from "preact";
import { useEffect, useId, useRef } from "preact/hooks";
import { X } from "lucide-preact";

export function Page({ title, lede, actions, children, wide = false }: { title: string; lede?: ComponentChildren; actions?: ComponentChildren; children: ComponentChildren; wide?: boolean }) {
  return (
    <div class={`page${wide ? " page-wide" : ""}`}>
      <header class="page-head" data-geo="page-head">
        <div class="page-heading">
          <h1 tabIndex={-1}>{title}</h1>
          {lede && <p class="page-lede">{lede}</p>}
        </div>
        {actions && <div class="page-actions" data-geo="page-actions">{actions}</div>}
      </header>
      {children}
    </div>
  );
}

export function Section({ title, description, children, aside }: { title?: string; description?: ComponentChildren; children: ComponentChildren; aside?: ComponentChildren }) {
  const id = useId();
  return (
    <section class="section" aria-labelledby={title ? id : undefined}>
      {(title || aside) && (
        <div class="section-head">
          <div>
            {title && <h2 id={id}>{title}</h2>}
            {description && <p class="section-description">{description}</p>}
          </div>
          {aside}
        </div>
      )}
      {children}
    </section>
  );
}

/** Settings row: what it is on the left, the control on the right. */
export function Row({ icon, title, description, control, children, id }: { icon?: ComponentChildren; title: ComponentChildren; description?: ComponentChildren; control?: ComponentChildren; children?: ComponentChildren; id?: string }) {
  return (
    <div class="row" id={id}>
      {icon && <span class="row-icon">{icon}</span>}
      <div class="row-text">
        <div class="row-title">{title}</div>
        {description && <div class="row-description">{description}</div>}
        {children}
      </div>
      {control && <div class="row-control">{control}</div>}
    </div>
  );
}

export function Rows({ children }: { children: ComponentChildren }) {
  return <div class="rows">{children}</div>;
}

export function Card({ children, class: className, tone }: { children: ComponentChildren; class?: string; tone?: "sunken" | "tile" }) {
  return <div class={`card${tone ? ` card-${tone}` : ""}${className ? ` ${className}` : ""}`}>{children}</div>;
}

export function Callout({ tone = "info", icon, title, children, action }: { tone?: "info" | "warn" | "error" | "ok"; icon?: ComponentChildren; title: ComponentChildren; children?: ComponentChildren; action?: ComponentChildren }) {
  return (
    <div class={`callout callout-${tone}`} role={tone === "error" ? "alert" : undefined}>
      {icon && <span class="callout-icon">{icon}</span>}
      <div class="callout-body">
        <div class="callout-title">{title}</div>
        {children && <div class="callout-text">{children}</div>}
      </div>
      {action && <div class="callout-action">{action}</div>}
    </div>
  );
}

export function EmptyState({ art, title, children, action }: { art?: ComponentChildren; title: string; children?: ComponentChildren; action?: ComponentChildren }) {
  return (
    <div class="empty">
      {art && <div class="empty-art">{art}</div>}
      <h3>{title}</h3>
      {children && <p>{children}</p>}
      {action && <div class="empty-action">{action}</div>}
    </div>
  );
}

const FOCUSABLE = 'button:not([disabled]),[href],input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';

/** Modal dialog: traps focus, closes on Escape or backdrop, restores focus on close. */
export function Dialog({ title, description, open, onClose, children, footer, size = "md" }: { title: string; description?: ComponentChildren; open: boolean; onClose: () => void; children?: ComponentChildren; footer?: ComponentChildren; size?: "md" | "lg" }) {
  const panel = useRef<HTMLDivElement>(null);
  const titleId = useId();
  useEffect(() => {
    if (!open) return;
    const previous = document.activeElement as HTMLElement | null;
    const node = panel.current;
    const first = node?.querySelector<HTMLElement>("[data-autofocus]") ?? node?.querySelector<HTMLElement>(FOCUSABLE);
    first?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      } else if (event.key === "Tab" && node) {
        const items = [...node.querySelectorAll<HTMLElement>(FOCUSABLE)];
        const firstItem = items[0];
        const lastItem = items[items.length - 1];
        if (!firstItem || !lastItem) return;
        if (event.shiftKey && document.activeElement === firstItem) {
          event.preventDefault();
          lastItem.focus();
        } else if (!event.shiftKey && document.activeElement === lastItem) {
          event.preventDefault();
          firstItem.focus();
        }
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      previous?.focus?.();
    };
  }, [open]);
  if (!open) return null;
  return (
    <div class="dialog-layer" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <div class={`dialog dialog-${size}`} role="dialog" aria-modal="true" aria-labelledby={titleId} ref={panel}>
        <header class="dialog-head">
          <div>
            <h2 id={titleId}>{title}</h2>
            {description && <p class="dialog-description">{description}</p>}
          </div>
          <button type="button" class="icon-button" aria-label="Close" onClick={onClose}>
            <X size={18} />
          </button>
        </header>
        {children && <div class="dialog-body">{children}</div>}
        {footer && <footer class="dialog-foot">{footer}</footer>}
      </div>
    </div>
  );
}
