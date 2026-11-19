import { createHash } from "node:crypto";

const FIRST_CHECKOUT_PORT = 20_000;
const CHECKOUT_PORT_COUNT = 10_000;
const LAST_TCP_PORT = 65_535;

/** Give each checkout a stable dev-server address. A collision fails before any test runs. */
export function browserTestPort(checkout: string, override?: string): number {
  if (override === undefined) {
    const hash = createHash("sha256").update(checkout).digest().readUInt32BE(0);
    return FIRST_CHECKOUT_PORT + hash % CHECKOUT_PORT_COUNT;
  }
  const port = Number(override);
  if (!/^\d+$/.test(override) || port < 1 || port > LAST_TCP_PORT) throw new Error("UI_PORT must be a TCP port from 1 to 65535.");
  return port;
}
