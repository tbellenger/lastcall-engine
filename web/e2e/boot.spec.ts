import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const boot = readFileSync(fileURLToPath(new URL("../../fixtures/sample/tests/boot.sha256", import.meta.url)), "utf8").trim();

test("boot frame matches the native hash and a command updates narration", async ({ page }) => {
  await page.goto("/");
  const canvas = page.locator("canvas");
  await expect(canvas).toHaveAttribute("data-ready", "1", { timeout: 15000 });
  await expect(canvas).toHaveAttribute("data-frame-hash", boot);
  await expect(page.locator("#live")).toContainText("Night settles on the old gatehouse porch.");

  await page.locator("body").click();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Enter");
  await page.locator("#line").fill("look at the rim");
  await page.keyboard.press("Enter");
  await expect(page.locator("#live")).toContainText("stone well");
});
