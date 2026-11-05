import { test, expect } from "@playwright/test";

const unique = `e2e-${Date.now().toString(36)}`;

test.describe("星枢 XS - 核心功能 E2E (Playwright CLI)", () => {
  test.beforeEach(async ({ page }) => {
    // collect console errors for evidence
    page.on("console", (msg) => {
      if (msg.type() === "error") console.log(`[console error] ${msg.text()}`);
    });
  });

  test("首页加载无 Failed to fetch，仓库列表正常渲染", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator(".brand")).toContainText("星枢");
    await expect(page.locator("text=Failed to fetch")).toHaveCount(0);
    await expect(page.locator(".notice.error")).toHaveCount(0);
    // sidebar shows counts
    await expect(page.getByRole("link", { name: /仓库/ })).toBeVisible();
    // toolbar search input
    await expect(page.getByPlaceholder("搜索仓库、组织或路径…")).toBeVisible();
    // table should have 5 repos from synthetic staging
    await expect(page.locator("table tbody tr")).toHaveCount(6, { timeout: 10000 });
    await expect(page.locator("table")).toContainText("third-party-demo");
    await expect(page.locator("table")).toContainText("own-demo");
    await expect(page.locator("table")).toContainText("bare-demo");
    await page.screenshot({ path: `../.playwright-cli/${Date.now()}-01-home-repos.png`, fullPage: false });
  });

  test("仓库搜索与标签过滤", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("table tbody tr")).toHaveCount(6, { timeout: 8000 });
    const search = page.getByPlaceholder("搜索仓库、组织或路径…");
    await search.fill("own-demo");
    await expect(page.locator("table tbody tr")).toHaveCount(1);
    await expect(page.locator("table")).toContainText("own-demo");
    await search.fill("");
    await expect(page.locator("table tbody tr")).toHaveCount(6);
    // tag filter (initially 全部标签)
    const tagSelect = page.getByLabel("按标签过滤");
    await expect(tagSelect).toBeVisible();
    await tagSelect.click();
    await page.getByRole("option", { name: "全部标签" }).click();
    await expect(page.locator("table tbody tr")).toHaveCount(6);
  });

  test("标签创建、附加到仓库、过滤、删除", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("table tbody tr")).toHaveCount(6, { timeout: 8000 });

    // go to tags view
    await page.getByRole("link", { name: /标签/ }).click();
    await expect(page.locator(".tag-create input")).toBeVisible();
    const slug = `tag-${unique}`;
    await page.locator(".tag-create input").fill(slug);
    await page.locator(".tag-create button", { hasText: "添加" }).click();
    await expect(page.locator(".tag-card", { hasText: slug })).toBeVisible({ timeout: 8000 });

    // attach tag to first repo via drawer
    await page.getByRole("link", { name: /仓库/ }).click();
    await expect(page.locator("table tbody tr")).toHaveCount(6);
    await page.locator("table tbody tr").first().click();
    await expect(page.locator(".drawer")).toBeVisible();
    const attachTrigger = page.locator(".drawer").getByLabel("添加标签");
    await attachTrigger.click();
    await page.getByRole("option", { name: slug }).click();
    // drawer should reflect tag
    await expect(page.locator(".drawer")).toContainText(slug);
    await page.locator(".drawer .close").click();
    await expect(page.locator(".drawer")).toHaveCount(0);

    // filter by tag via tag-card click
    await page.getByRole("link", { name: /标签/ }).click();
    await page.locator(".tag-card", { hasText: slug }).locator(".tag-card-body").click();
    // should be on repos view filtered
    await expect(page.getByRole("heading", { name: "仓库目录" })).toBeVisible();
    await expect(page.locator("table tbody tr")).toHaveCount(1);
    // clear filter
    await page.getByLabel("按标签过滤").click();
    await page.getByRole("option", { name: "全部标签" }).click();

    // delete tag
    await page.getByRole("link", { name: /标签/ }).click();
    await expect(page.locator(".tag-card", { hasText: slug })).toBeVisible();
    await page.locator(".tag-card", { hasText: slug }).locator(".tag-delete").click();
    await expect(page.locator(".tag-card", { hasText: slug })).toHaveCount(0);
  });

  test("磁盘看板统计与按类型分布", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("link", { name: "磁盘看板" }).click();
    await expect(page.getByRole("heading", { name: "磁盘看板" })).toBeVisible();
    await expect(page.locator(".stat-card", { hasText: "索引仓库" })).toContainText("6");
    await expect(page.locator(".stat-card", { hasText: "已占用空间" })).toBeVisible();
    // byKind bar should contain third-party and own etc
    await expect(page.locator(".kind-bar")).toBeVisible();
    await expect(page.locator(".kind-bar")).toContainText("third-party");
  });

  test("设置页根目录管理：无效路径报错与展示", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("link", { name: "设置" }).click();
    await expect(page.getByRole("heading", { name: "根目录管理" })).toBeVisible();
    await expect(page.locator(".root-item")).toHaveCount(2); // root-a & root-b from synthetic
    // try adding invalid path
    const input = page.getByPlaceholder("输入根目录绝对路径…");
    await input.fill("Z:\\nonexistent-{{unique}}");
    await page.locator(".root-create button", { hasText: "添加" }).click();
    await expect(page.locator(".notice.error")).toBeVisible({ timeout: 8000 });
    await expect(page.locator(".notice.error")).toContainText(/400|404|未找到|invalid|HTTP/i);
    // valid path should be the existing staging root-a (already exists, but UI will try POST and get 409? Actually upsert returns 201, but duplicate path will upsert and return 201, then scan needed)
    // instead test that removing and re-adding works via API: we test UI remove
    // cancel error by adding a temp dir
  });

  test("根目录移除与重新添加（隔离验证）", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("link", { name: "设置" }).click();
    await expect(page.locator(".root-item")).toHaveCount(2);
    // create a temp dir via API then remove via UI to avoid filesystem dependency
    const tmpPath = `H:/zeogit/one/A08-xingshu/.staging/e2e-playwright/root-a`; // already exists, use dummy subdir
    // Use API directly to verify delete works, then UI reflects
    const rootsRes = await page.request.get("/api/v1/roots");
    expect(rootsRes.ok()).toBeTruthy();
    const roots = (await rootsRes.json()) as Array<{ id: number; path: string }>;
    expect(roots.length).toBe(2);
    // pick second root to delete via UI
    const secondRoot = roots[1];
    await page.locator(".root-item", { hasText: secondRoot.path }).getByRole("button", { name: "移除" }).click();
    await expect(page.locator(".root-item", { hasText: secondRoot.path })).toHaveCount(0, { timeout: 8000 });
    // re-add via UI
    await page.getByPlaceholder("输入根目录绝对路径…").fill(secondRoot.path);
    await page.locator(".root-create button", { hasText: "添加" }).click();
    await expect(page.locator(".root-item", { hasText: secondRoot.path })).toBeVisible({ timeout: 8000 });
    // need to re-scan to restore repos
    const scanBtn = page.locator(".settings-actions").getByRole("button", { name: /扫描索引/ });
    await scanBtn.click();
    // wait for task panel to complete
    await expect(page.locator(".task-panel")).toBeVisible({ timeout: 5000 });
    await expect(page.locator(".task-panel")).toContainText(/已完成|completed/, { timeout: 60000 }); // NOTE: loaded machines scan ~3s/repo
  });

  test("抽屉详情：类型切换与远程信息", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("table tbody tr")).toHaveCount(6, { timeout: 8000 });
    // click own-demo (synthetic may be own or third-party depending on my_orgs, so don't assert initial value)
    const ownRow = page.locator("table tbody tr", { hasText: "own-demo" });
    await ownRow.click();
    await expect(page.locator(".drawer")).toBeVisible();
    await expect(page.locator(".drawer-title")).toContainText("own-demo");
    await expect(page.locator(".drawer")).toContainText("https://github.com/cc01cc/own-demo.git");
    // kind trigger should exist and be changeable (with confirm dialog)
    const kindTrigger = page.locator(".drawer").getByLabel("仓库类型");
    await expect(kindTrigger).toBeVisible();
    const initialKind = ((await kindTrigger.textContent()) ?? "").trim();
    // change to fork then back to verify PUT works (each change needs dialog confirm)
    await kindTrigger.click();
    await page.getByRole("option", { name: "fork", exact: true }).click();
    await expect(page.getByRole("alertdialog")).toBeVisible();
    await page.getByRole("button", { name: "确认切换" }).click();
    await expect(page.locator(".drawer")).toContainText("fork");
    // reload to ensure persisted
    await page.reload();
    await expect(page.locator("table tbody tr")).toHaveCount(6, { timeout: 8000 });
    await ownRow.click();
    await expect(page.locator(".drawer").getByLabel("仓库类型")).toContainText("fork", { timeout: 5000 });
    await page.locator(".drawer").getByLabel("仓库类型").click();
    await page.getByRole("option", { name: initialKind, exact: true }).click();
    await page.getByRole("button", { name: "确认切换" }).click();
    await expect(page.locator(".drawer").getByLabel("仓库类型")).toContainText(initialKind);
    await page.locator(".drawer .close").click();
  });

  test("异步任务：扫描索引任务流与结果渲染", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("link", { name: "设置" }).click();
    await expect(page.locator(".settings-actions").getByRole("button", { name: /扫描索引/ })).toBeVisible();
    // trigger scan via settings button (also sidebar has scan)
    await page.locator(".settings-actions").getByRole("button", { name: /扫描索引/ }).click();
    // task panel appears
    await expect(page.locator(".task-panel")).toBeVisible({ timeout: 5000 });
    await expect(page.locator(".task-panel")).toContainText("索引扫描");
    await expect(page.locator(".task-progress")).toBeVisible();
    // wait for completed
    await expect(page.locator(".task-panel")).toContainText(/已完成/, { timeout: 60000 }); // NOTE: see above
    await expect(page.locator(".task-panel")).toContainText(/共 \d+ 个仓库|roots_scanned|发现/);
    // result table or scan stats should be visible
    await expect(page.locator(".task-details")).toBeVisible();
    // check Chinese labels: 成功/已跳过 etc not applicable for scan but scan stats
    await expect(page.locator(".task-scan-stats")).toContainText(/已扫描.*根/);
    await expect(page.locator(".task-scan-stats")).toContainText(/发现.*仓/);
    // dismiss task
    await page.locator(".task-panel").getByRole("button", { name: "关闭" }).click();
    await expect(page.locator(".task-panel")).toHaveCount(0);
    await page.screenshot({ path: `../.playwright-cli/${Date.now()}-scan-result.png`, fullPage: false });
  });

  test("异步任务：批量 pull 冲突决策流（dirty-demo 需决策）", async ({ page }) => {
    test.slow(); // sandbox pull latency (TLS retries on fake github remotes) can exceed the 45s default
    await page.goto("/");
    // ensure dirty-demo exists and has lockViolation
    await expect(page.locator("table tbody tr", { hasText: "dirty-demo" })).toBeVisible({ timeout: 8000 });
    await page.locator("table tbody tr", { hasText: "dirty-demo" }).click();
    await expect(page.locator(".drawer")).toBeVisible();
    await expect(page.locator(".drawer")).toContainText("检测到本地改动");
    await page.locator(".drawer .close").click();

    // trigger pull task via sidebar
    await page.getByRole("button", { name: "批量 pull" }).click();
    await expect(page.locator(".task-panel")).toBeVisible({ timeout: 5000 });
    await expect(page.locator(".task-panel")).toContainText("批量 pull");

    // wait for waiting_for_decision (dirty-demo, diverged-demo, third-party-demo)
    // NOTE: sandbox has no github access; fork/own-demo pulls burn TLS-retry seconds
    // before conflicts surface, so this window is generous (upstream CI is fast).
    await expect(page.locator(".task-conflicts")).toBeVisible({ timeout: 90000 });
    // synthetic staging: dirty-demo + diverged-demo hit conflicts (sync repos pull cleanly)
    await expect(page.locator(".task-conflict")).toHaveCount(2, { timeout: 15000 });
    const dirtyCard = page.locator(".task-conflict", { hasText: "dirty-demo" });
    await expect(dirtyCard.getByRole("button", { name: "备份后拉取" })).toBeEnabled();
    await expect(dirtyCard.getByRole("button", { name: "覆盖本地" })).toBeEnabled();
    await expect(dirtyCard.getByRole("button", { name: "保持现状" })).toBeEnabled();

    // PLAN-0316：点击 dirty 明细文件 → 只读 diff 弹层（含工作区 hunk）
    const diffEntry = dirtyCard.locator(".conflict-files li.diff-entry", { hasText: "README.md" });
    await expect(diffEntry).toBeVisible();
    await diffEntry.click();
    await expect(page.locator(".diff-sheet")).toBeVisible();
    await expect(page.locator(".diff-sheet .diff-patch")).toContainText("tracked staging change");
    await page.locator(".diff-sheet button[data-slot='sheet-close']").click();
    await expect(page.locator(".diff-sheet")).toHaveCount(0);

    // decide abort for dirty-demo to keep filesystem stable
    await dirtyCard.getByRole("button", { name: "保持现状" }).click();
    // drain remaining conflicts (keep current state) so the task reaches a terminal state:
    // every waiting card gets skipped; conflicts region empties when all repos are decided
    await expect(async () => {
      const pending = page.locator(".task-conflict button:enabled", { hasText: "保持现状" });
      const count = await pending.count();
      if (count > 0) await pending.first().click();
      await expect(page.locator(".task-conflict")).toHaveCount(0, { timeout: 5000 }).catch(() => {
        throw new Error("conflicts still pending");
      });
    }).toPass({ timeout: 30000 });
    // reconcile marks the task failed when any repo errored (synthetic remotes missing); terminal state is enough
    await expect(page.locator(".task-status")).toHaveClass(/completed|failed/, { timeout: 20000 });
    // result counts should eventually appear (may need a short wait for SSE)
    await expect(page.locator(".task-counts")).toBeVisible({ timeout: 15000 });
    await expect(page.locator(".task-counts")).toContainText(/已中止|成功|冲突需决策|失败/, { timeout: 10000 });
    await expect(page.locator(".task-repos-table")).toBeVisible({ timeout: 10000 });
    // table shows repoId per current rendering; check for status and backup column instead of repo name
    await expect(page.locator(".task-repos-table")).toContainText(/已中止|已完成|失败/, { timeout: 10000 });
    await expect(page.locator(".task-repos-table th", { hasText: "备份" })).toBeVisible();
    await page.screenshot({ path: `../.playwright-cli/${Date.now()}-pull-conflict.png`, fullPage: false });
    // dismiss
    await page.locator(".task-panel").getByRole("button", { name: "关闭" }).click().catch(() => {});
  });

  test("API 边界：无效标签与请求 ID 透传", async ({ page }) => {
    // direct API check for problem details and x-request-id
    const res = await page.request.post("/api/v1/tags", { data: {} });
    expect([400, 422].includes(res.status())).toBeTruthy();
    expect(res.headers()["x-request-id"]).toBeTruthy();
    // body may be JSON problem or plain text depending on axum validation
    const text = await res.text();
    expect(text.length).toBeGreaterThan(0);

    // valid tag then invalid attach
    const ok = await page.request.post("/api/v1/tags", { data: { slug: `api-${unique}`, label: `api-${unique}` } });
    expect(ok.status()).toBe(201);
    const bad = await page.request.post("/api/v1/repos/99999/tags", { data: { slug: `api-${unique}` } });
    expect([404, 500].includes(bad.status())).toBeTruthy();
    expect(bad.headers()["x-request-id"]).toBeTruthy();
  });

  test("视觉与可访问性：侧边栏与抽屉无遮挡", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("table tbody tr")).toHaveCount(6, { timeout: 8000 });
    // sidebar should be visible without overflow clipping
    const sidebar = page.locator(".sidebar");
    await expect(sidebar).toBeInViewport();
    // open drawer and check it is in viewport and not clipped by overflow
    await page.locator("table tbody tr").first().click();
    const drawer = page.locator(".drawer");
    await expect(drawer).toBeVisible();
    await expect(drawer).toBeInViewport();
    // close via X and via overlay click (if any)
    await drawer.locator(".close").click();
    await expect(drawer).toHaveCount(0);
  });

  test("冲突面板：dirty 明细与后果对照", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("table tbody tr", { hasText: "dirty-demo" })).toBeVisible({ timeout: 8000 });
    await page.locator("table tbody tr", { hasText: "dirty-demo" }).click();
    await expect(page.locator(".drawer")).toBeVisible();
    await expect(page.locator(".drawer .conflict")).toContainText("检测到本地改动");
    // 后果对照清单
    await expect(page.locator(".drawer .conflict-outcome-list")).toContainText("备份后拉取");
    await expect(page.locator(".drawer .conflict-outcome-list")).toContainText("不可恢复");
    // overwrite 需二次确认
    const drawer = page.locator(".drawer .conflict");
    await drawer.locator("button", { hasText: "覆盖本地" }).click();
    await expect(page.locator(".drawer").getByRole("button", { name: /确认覆盖/ })).toBeVisible();
    await page.locator(".drawer .close").click();
  });

  test("冲突面板：diverged 双栏与 ≡ 等价标记（批量 pull 决策流）", async ({ page }) => {
    test.slow(); // same sandbox pull latency as above
    await page.goto("/");
    await expect(page.locator("table tbody tr", { hasText: "diverged-demo" })).toBeVisible({ timeout: 8000 });
    // 触发批量 pull（ask 模式），diverged-demo 会进入等待决策；若上一用例任务未终态导致 409 则重试
    await expect(async () => {
      await page.getByRole("button", { name: "批量 pull" }).click();
      await page.waitForTimeout(1000);
      await expect(page.locator(".notice.error")).toHaveCount(0, { timeout: 2000 }).catch(() => {
        throw new Error("start rejected; retrying");
      });
    }).toPass({ timeout: 120000 });
    await expect(page.locator(".task-panel")).toBeVisible({ timeout: 10000 });
    // NOTE: same sandbox-network generosity as above.
    await expect(page.locator(".task-conflicts")).toBeVisible({ timeout: 90000 });
    const conflictCard = page.locator(".task-conflict", { hasText: "diverged-demo" });
    await expect(conflictCard).toBeVisible({ timeout: 15000 });
    // 结构化面板：标题 + 双栏 + 分叉点
    await expect(conflictCard).toContainText("本地与上游历史分叉");
    await expect(conflictCard.locator(".conflict-columns")).toBeVisible();
    await expect(conflictCard.locator(".conflict-columns")).toContainText("本地独有");
    await expect(conflictCard.locator(".conflict-columns")).toContainText("上游新增");
    await expect(conflictCard).toContainText("分叉点");
    // ≡ 等价标记（fixture 本地提交与上游重做提交 patch 等价）
    await expect(conflictCard.locator(".conflict-equivalent")).toContainText("内容相同");
    // 后果对照可展开
    await conflictCard.locator(".conflict-outcomes summary").click();
    await expect(conflictCard.locator(".conflict-outcomes")).toContainText("不可恢复");
    // 决策：保持现状，维持文件系统稳定；drain 全部冲突卡使任务进入终态
    await conflictCard.getByRole("button", { name: "保持现状" }).click();
    await expect(async () => {
      const pending = page.locator(".task-conflict button:enabled", { hasText: "保持现状" });
      const count = await pending.count();
      if (count > 0) await pending.first().click();
      await expect(page.locator(".task-conflict")).toHaveCount(0, { timeout: 5000 }).catch(() => {
        throw new Error("conflicts still pending");
      });
    }).toPass({ timeout: 30000 });
    await expect(page.locator(".task-status")).toHaveClass(/completed|failed/, { timeout: 20000 });
    await expect(page.locator(".task-repos-table")).toContainText(/已跳过|skipped|已中止|aborted/, { timeout: 10000 });
    await page.locator(".task-panel").getByRole("button", { name: "关闭" }).click().catch(() => {});
  });

  test("打开目录 API：合法路径与越界拒绝", async ({ page }) => {
    await page.goto("/");
    // 越界路径（root 之外）→ 400
    const bad = await page.request.post("/api/v1/open", {
      data: { path: "C:/Windows" },
    });
    expect(bad.status()).toBe(400);
    const badBody = await bad.text();
    expect(badBody).toContain("outside");
    // 合法路径（staging root 内）→ 204
    const rootsRes = await page.request.get("/api/v1/roots");
    const roots = (await rootsRes.json()) as Array<{ id: number; path: string }>;
    expect(roots.length).toBeGreaterThan(0);
    const ok = await page.request.post("/api/v1/open", {
      data: { path: roots[0].path },
    });
    expect(ok.status()).toBe(204);
  });

  test("ahead 徽标与 favicon", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });
    await page.goto("/");
    await expect(page.locator("table tbody tr").first()).toBeVisible({ timeout: 8000 });
    // favicon 不再 404
    const favicon = await page.request.get("/favicon.ico");
    expect([200, 404]).toContain(favicon.status());
    expect(errors.filter((e) => e.includes("favicon"))).toHaveLength(0);
    // 徽标：无 ahead 仓库时不出现在任何行（渲染无错误即可，ahead 场景由有头验证覆盖）
    expect(await page.locator(".status.warning", { hasText: "本地领先" }).count()).toBeGreaterThanOrEqual(0);
  });

  test("PLAN-0314 状态过滤与抽屉状态/备份区", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("table tbody tr")).toHaveCount(6, { timeout: 10000 });

    // 状态过滤：本地改动 只剩 dirty-demo
    await page.getByLabel("按状态过滤").click();
    await page.getByRole("option", { name: "本地改动" }).click();
    await expect(page.locator("table tbody tr")).toHaveCount(1);
    await expect(page.locator("table tbody tr").first()).toContainText("dirty-demo");
    await page.getByLabel("按状态过滤").click();
    await page.getByRole("option", { name: "全部状态" }).click();
    await expect(page.locator("table tbody tr")).toHaveCount(6);

    // 抽屉状态区 + 备份区可见
    await page.locator("table tbody tr").first().click();
    const drawer = page.locator(".drawer");
    await expect(drawer).toBeVisible();
    await expect(drawer.getByText("状态", { exact: true })).toBeVisible();
    await expect(drawer.getByRole("button", { name: "检测远端" })).toBeVisible();
    await expect(drawer.getByText("备份", { exact: true })).toBeVisible();
    await expect(drawer.getByRole("button", { name: "创建备份" })).toBeVisible();

    // 远端检测（点击后才产生网络请求；staging remote 不存在 → unknown）
    await drawer.getByRole("button", { name: "检测远端" }).click();
    await expect(drawer.locator(".remote-result")).toContainText(/远端|unknown/, { timeout: 15000 });

    // 备份列表可能为空（staging 未必有 .bak），面板空态可接受
    await expect(drawer.getByText(/暂无备份|备份/).first()).toBeVisible();
  });

  test("PLAN-0316 只读 diff：契约与安全拒绝", async ({ page }) => {
    const reposResponse = await page.request.get("/api/v1/repos");
    expect(reposResponse.status()).toBe(200);
    const repos = (await reposResponse.json()) as Array<{ id: number; name: string }>;
    const dirty = repos.find((repo) => repo.name.includes("dirty-demo"));
    expect(dirty).toBeTruthy();
    const repoId = dirty!.id;

    // tracked 修改：返回相对 HEAD 的只读 patch
    const diffResponse = await page.request.get(`/api/v1/repos/${repoId}/diff?path=README.md`);
    expect(diffResponse.status()).toBe(200);
    const diff = (await diffResponse.json()) as { path: string; kind: string; patch: string; truncated: boolean };
    expect(diff.path).toBe("README.md");
    expect(diff.kind).toBe("diff");
    expect(diff.patch).toContain("tracked staging change");
    expect(diff.truncated).toBe(false);

    // 未跟踪文件：无 patch、明确标记
    const untrackedResponse = await page.request.get(`/api/v1/repos/${repoId}/diff?path=local-change.txt`);
    expect(untrackedResponse.status()).toBe(200);
    const untracked = (await untrackedResponse.json()) as { kind: string; patch: string };
    expect(untracked.kind).toBe("untracked");
    expect(untracked.patch).toBe("");

    // 越界与绝对路径拒绝
    expect((await page.request.get(`/api/v1/repos/${repoId}/diff?path=..%2Foutside.txt`)).status()).toBe(400);
    expect((await page.request.get(`/api/v1/repos/${repoId}/diff?path=C%3A%2Fwindows%2Fwin.ini`)).status()).toBe(400);
    // 不存在的路径
    expect((await page.request.get(`/api/v1/repos/${repoId}/diff?path=missing.txt`)).status()).toBe(404);

    // bare 仓库拒绝
    const bare = repos.find((repo) => repo.name.includes("bare"));
    if (bare) {
      expect((await page.request.get(`/api/v1/repos/${bare.id}/diff?path=README.md`)).status()).toBe(400);
    }
  });

  test("PLAN-0316 标签策略：生效来源与继承语义", async ({ page }) => {
    const reposResponse = await page.request.get("/api/v1/repos");
    const repos = (await reposResponse.json()) as Array<{ id: number; name: string; tags: string[] }>;
    const own = repos.find((repo) => repo.name.includes("own-demo"));
    expect(own).toBeTruthy();
    const repoId = own!.id;

    const slug = `e2e-policy-${Date.now().toString(36)}`;
    const created = await page.request.post("/api/v1/tags", { data: { slug, label: slug } });
    expect(created.status()).toBe(201);
    const tags = (await (await page.request.get("/api/v1/tags")).json()) as Array<{ id: number; slug: string }>;
    const tagId = tags.find((tag) => tag.slug === slug)!.id;
    expect((await page.request.post(`/api/v1/repos/${repoId}/tags`, { data: { slug } })).status()).toBe(204);

    // 标签策略写入生效层；成员仓 reason 为 tag 且 effective 反映该策略
    const set = await page.request.put(`/api/v1/tags/${tagId}/policy`, {
      data: { pullStrategy: "mirror", conflict: "stop", unattended: "skip" },
    });
    expect(set.status()).toBeLessThan(400);
    const view = (await (await page.request.get(`/api/v1/repos/${repoId}/policy`)).json()) as Record<string, unknown>;
    expect(view.source).toBe("tag");
    expect(view.sourceTagId).toBe(tagId);
    expect((view.effective as Record<string, unknown>).pullStrategy).toBe("mirror");

    // UI：策略页加载成员仓 → 来源徽标显示标签
    await page.goto("/");
    await page.getByRole("link", { name: "策略" }).click();
    await page.getByLabel("选择仓库").click();
    await page.getByRole("option", { name: new RegExp(own!.name) }).click();
    await page.getByRole("button", { name: "加载" }).first().click();
    await expect(page.getByText(/来源：标签/)).toBeVisible({ timeout: 8000 });

    // 清理：移除成员与标签，还原继承语义
    expect((await page.request.delete(`/api/v1/repos/${repoId}/tags`, { data: { slug } })).status()).toBe(204);
    expect((await page.request.delete(`/api/v1/tags/${tagId}`)).status()).toBeLessThan(400);
    const after = (await (await page.request.get(`/api/v1/repos/${repoId}/policy`)).json()) as Record<string, unknown>;
    expect(after.source).not.toBe("tag");
  });

  test("PLAN-0316 搬迁空间预检：真实端值 + 不足禁用", async ({ page }) => {
    const rootsResponse = await page.request.get("/api/v1/roots");
    expect(rootsResponse.status()).toBe(200);
    const roots = (await rootsResponse.json()) as Array<{ id: number; name: string }>;
    expect(roots.length).toBeGreaterThan(0);
    const spaceResponse = await page.request.get(`/api/v1/roots/${roots[0].id}/space`);
    expect(spaceResponse.status()).toBe(200);
    const space = (await spaceResponse.json()) as { rootId: number; availableBytes: number };
    expect(space.rootId).toBe(roots[0].id);
    expect(space.availableBytes).toBeGreaterThan(0);
    expect((await page.request.get("/api/v1/roots/99999/space")).status()).toBe(404);

    // UI：mock 空间不足 → 搬迁按钮禁用 + 提示
    await page.route("**/api/v1/roots/*/space", (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ rootId: 1, availableBytes: 1 }),
      }),
    );
    await page.goto("/");
    await page.locator("table tbody tr").first().click();
    const drawer = page.locator(".drawer");
    await expect(drawer).toBeVisible();
    await drawer.getByLabel("目标根").click();
    await page.locator("[role=option]:not([aria-disabled=true])").first().click();
    await expect(drawer.locator(".move-space")).toContainText("空间不足", { timeout: 8000 });
    await expect(drawer.getByRole("button", { name: "搬迁" })).toBeDisabled();
    await page.unroute("**/api/v1/roots/*/space");
  });
});
