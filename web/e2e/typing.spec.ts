import { expect, test } from "@playwright/test";

test.use({ baseURL: "http://127.0.0.1:4174" });

async function clickBox(page: import("@playwright/test").Page, selector: string, dx = 0.5, dy = 0.5) {
  const box = await page.locator(selector).boundingBox();
  if (!box) {
    throw new Error(`${selector} has no box`);
  }
  await page.mouse.click(box.x + box.width * dx, box.y + box.height * dy);
}

test("typing works after a real click on the picture and under it", async ({ page }) => {
  await page.goto("/games/charlie?engine=next");
  const canvas = page.locator("#screen");
  const line = page.locator("#line");
  const live = page.locator("#live");
  await expect(canvas).toHaveAttribute("data-ready", "1", { timeout: 30000 });
  await expect(page.locator("#title")).toBeVisible();

  await clickBox(page, "#title");
  await page.keyboard.type("hi");
  await expect(line).toHaveValue("hi");

  for (let i = 0; i < 7; i += 1) {
    await page.keyboard.press("Enter");
  }
  await expect(line).toHaveValue("");

  await clickBox(page, "#screen", 0.5, 0.45);
  await page.keyboard.type("look");
  await expect(line).toHaveValue("look");
  await page.keyboard.press("Enter");
  await expect(live).toContainText("dingy bar", { timeout: 10000 });

  await clickBox(page, "#live");
  await page.keyboard.type("x");
  await expect(line).toHaveValue("x");

  await clickBox(page, "#below");
  await page.evaluate(() => {
    const active = document.activeElement;
    if (active instanceof HTMLElement) {
      active.blur();
    }
  });
  await page.keyboard.type("y");
  await page.keyboard.press("Backspace");
  await page.keyboard.type("z");
  await expect(line).toHaveValue("xz");

  await clickBox(page, "#site-search");
  await page.keyboard.type("abc");
  await expect(page.locator("#site-search")).toHaveValue("abc");
  await expect(line).toHaveValue("xz");
});
