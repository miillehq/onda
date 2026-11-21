import { expect, it } from "vitest";
import config from "../../playwright.config";
import { browserTestPort } from "../ui/server";

it("starts a dedicated server for this checkout and rejects invalid overrides", () => {
  expect(config.webServer).toMatchObject({ reuseExistingServer: false, url: config.use?.baseURL });
  expect(browserTestPort("/work/review")).not.toBe(browserTestPort("/work/release"));
  expect(browserTestPort("/work/review", "23001")).toBe(23001);
  for (const value of ["", "0", "1420other", "65536"]) expect(() => browserTestPort("/work", value)).toThrow("UI_PORT");
});
