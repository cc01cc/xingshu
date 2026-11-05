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

  // PLAN-0314 V6/V9：抽屉状态区 + 备份面板全流程（创建→列表→清理预览），证据入 evidence/
  test.describe("PLAN-0314 状态与备份面板", () => {
    test.use({ viewport: { width: 1600, height: 1000 } });

    test("抽屉状态区与备份面板全流程截图", async ({ page }) => {
      const stamp = Date.now();
      await page.goto("/");
      await expect(page.locator("table tbody tr").first()).toBeVisible({ timeout: 10000 });
      await page.locator("table tbody tr").first().click();
      const drawer = page.locator(".drawer");
      await expect(drawer).toBeVisible();
      // 状态区（本地快照 + 远端检测按钮）
      await expect(drawer.getByText("状态", { exact: true })).toBeVisible();
      await expect(drawer.getByRole("button", { name: "检测远端" })).toBeVisible();
      await drawer.getByRole("button", { name: "检测远端" }).click();
      await expect(drawer.locator(".remote-result")).toBeVisible({ timeout: 15000 });
      await page.screenshot({ path: `evidence/${stamp}-01-status.png`, fullPage: false });
      // 备份面板：创建两份 → keep=1 预览将删 1 项
      await drawer.getByRole("button", { name: "创建备份" }).click();
      await expect(drawer.locator(".backup-row").first()).toBeVisible({ timeout: 20000 });
      const rowCount = await drawer.locator(".backup-row").count();
      if (rowCount < 2) {
        await drawer.getByRole("button", { name: "创建备份" }).click();
        await expect(drawer.locator(".backup-row")).toHaveCount(2, { timeout: 20000 });
      }
      // 清理预览：keep 调为 1 → 超出的那份进入将删清单
      await drawer.getByRole("button", { name: "清理…" }).click();
      await expect(drawer.locator(".prune-preview")).toBeVisible({ timeout: 20000 });
      await drawer.locator(".prune-keep-input").fill("1");
      await drawer.getByRole("button", { name: "重新预览" }).click();
      await expect(drawer.locator(".prune-preview .prune-path").first()).toBeVisible({ timeout: 20000 });
      await page.screenshot({ path: `evidence/${stamp}-02-backup-prune.png`, fullPage: false });
      // 取消预览，避免误删（全流程删除由 V4 API 用例覆盖）
      await drawer.getByRole("button", { name: "取消" }).click();
      await expect(drawer.locator(".prune-preview")).toHaveCount(0);
    });
  });
});
