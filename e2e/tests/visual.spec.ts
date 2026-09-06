import { test, expect } from "@playwright/test";

// PLAN-263 视觉回归基线：列表 / 抽屉（含拉取历史展开，时间戳行 mask）/ 设置。
// fetch_log 时间戳随 staging 重建变化，故 mask .fetch-log-list 后比对。
// NOTE: baselines are headless-managed (headed rendering differs at pixel level);
// headed acceptance runs xingshu.spec.ts via `pnpm run test:headed:functional`.
test.describe("XS 视觉回归基线 (PLAN-263)", () => {
  test.use({ viewport: { width: 1920, height: 1080 } });

  test("仓库列表基线", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("table tbody tr")).toHaveCount(6, { timeout: 10000 });
    await expect(page).toHaveScreenshot("xs-repos-1920.png", { fullPage: false, animations: "disabled" });
  });

  test("详情抽屉（含拉取历史展开）基线", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("table tbody tr")).toHaveCount(6, { timeout: 10000 });
    await page.locator("table tbody tr").first().click();
    await expect(page.locator(".drawer")).toBeVisible();
    await page.locator(".fetch-log summary").click();
    await expect(page.locator(".fetch-log-list, .fetch-log .muted")).toBeVisible();
    await expect(page.locator(".drawer")).toHaveScreenshot("xs-drawer-1920.png", {
      animations: "disabled",
      mask: [page.locator(".fetch-log-list")],
    });
  });

  test("设置页基线", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("link", { name: "设置" }).click();
    await expect(page.getByRole("heading", { name: "设置" })).toBeVisible();
    // settings content renders skeletons while loading: wait for real content
    await expect(page.locator(".skeleton")).toHaveCount(0, { timeout: 15000 });
    await expect(page.locator(".root-create")).toBeVisible();
    await expect(page).toHaveScreenshot("xs-settings-1920.png", { fullPage: false, animations: "disabled" });
  });

  test("移动视口 375：导航与抽屉开合（一次性复核，不建基线）", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 });
    await page.goto("/");
    await expect(page.locator("table tbody tr").first()).toBeVisible({ timeout: 10000 });
    await page.locator(".hamburger").click();
    await expect(page.locator(".sidebar.open")).toBeVisible();
    await page.screenshot({ path: `../.playwright-cli/${Date.now()}-mobile-nav.png` });
    // backdrop center sits under the 224px sidebar: close via Escape instead
    await page.keyboard.press("Escape");
    await expect(page.locator(".sidebar.open")).toHaveCount(0);
    await page.locator("table tbody tr").first().click();
    await expect(page.locator(".drawer")).toBeVisible();
    await expect(page.locator(".drawer")).toBeInViewport();
    await page.screenshot({ path: `../.playwright-cli/${Date.now()}-mobile-drawer.png` });
  });
});
