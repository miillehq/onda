import { isNative } from "../api";
import "../styles/mock.css";

/**
 * Shown whenever the interface runs against the development mock (`npm run dev` in a browser),
 * so no screenshot or browser test can pass for the real app by mistake. Browser tests assert it
 * first; the real-shell suite asserts it is absent.
 */
export function MockBadge() {
  if (isNative) return null;
  return (
    <div class="mock-badge" data-testid="backend-badge" role="note">
      Development mock
    </div>
  );
}
