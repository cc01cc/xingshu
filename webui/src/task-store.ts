import { computed, onBeforeUnmount, ref } from "vue";

export type TaskType = "scan" | "pull";
export type TaskStatus = "pending" | "running" | "waiting_for_decision" | "completed" | "failed" | "interrupted";
export type TaskConnection = "idle" | "connecting" | "connected" | "reconnecting" | "closed" | "error";

export type Task = {
  taskId: string;
  type: TaskType;
  status: TaskStatus;
  conflictMode: "policy" | "ask";
  requestId: string | null;
  operationId: string;
  progressCurrent: number | null;
  progressTotal: number | null;
  lastRepo: string | null;
  lastResult: string | null;
  error: string | null;
  resultJson: string | null;
  createdAt: string;
  updatedAt: string;
};

export type ConflictDetail = {
  kind: "dirty" | "diverged" | "nonFf";
  reason: string;
  ahead: number;
  behind: number;
  mergeBase: string | null;
  mergeBaseDate: string | null;
  localOnly: Array<{ short: string; summary: string; author: string; date: string }>;
  upstreamOnly: Array<{ short: string; summary: string; author: string; date: string }>;
  equivalent: Array<[string, string]>;
  statusEntries: Array<{ index: string; worktree: string; path: string }>;
  staged: number;
  modified: number;
  untracked: number;
};

export type TaskConflict = {
  taskId: string;
  repoId: number;
  repoName: string;
  repoPath: string;
  status: "waiting_decision" | "resolving" | "failed" | "interrupted";
  conflictReason: string | null;
  conflictDetail: ConflictDetail | null;
  allowedActions: Array<"backup" | "overwrite" | "abort">;
  requestedAction: "backup" | "overwrite" | "abort" | null;
  result: string | null;
  durationMs: number | null;
  backupPath: string | null;
  error: string | null;
};

type TaskEvent = {
  taskId: string;
  repoId?: number | null;
  repoStatus?: string | null;
  conflictReason?: string | null;
  conflict?: ConflictDetail | null;
  requestedAction?: string | null;
  backupPath?: string | null;
  event?: string;
  current?: number | null;
  total?: number | null;
  lastRepo?: string | null;
  lastResult?: string | null;
  error?: string | null;
  durationMs?: number | null;
};

function parseConflictDetail(value: unknown): ConflictDetail | null {
  if (!isRecord(value)) return null;
  const kind = value.kind;
  if (kind !== "dirty" && kind !== "diverged" && kind !== "nonFf") return null;
  const briefs = (input: unknown) =>
    Array.isArray(input)
      ? input.filter(isRecord).map((item) => ({
          short: String(item.short ?? ""),
          summary: String(item.summary ?? ""),
          author: String(item.author ?? ""),
          date: String(item.date ?? ""),
        }))
      : [];
  const entries = Array.isArray(value.statusEntries)
    ? value.statusEntries.filter(isRecord).map((item) => ({
        index: String(item.index ?? ""),
        worktree: String(item.worktree ?? ""),
        path: String(item.path ?? ""),
      }))
    : [];
  const equivalent = Array.isArray(value.equivalent)
    ? value.equivalent
        .filter((pair): pair is [unknown, unknown] => Array.isArray(pair) && pair.length === 2)
        .map((pair) => [String(pair[0]), String(pair[1])] as [string, string])
    : [];
  const number = (input: unknown) => (typeof input === "number" && Number.isFinite(input) ? input : 0);
  return {
    kind,
    reason: String(value.reason ?? ""),
    ahead: number(value.ahead),
    behind: number(value.behind),
    mergeBase: asNullableString(value.mergeBase),
    mergeBaseDate: asNullableString(value.mergeBaseDate),
    localOnly: briefs(value.localOnly),
    upstreamOnly: briefs(value.upstreamOnly),
    equivalent,
    statusEntries: entries,
    staged: number(value.staged),
    modified: number(value.modified),
    untracked: number(value.untracked),
  };
}

const TERMINAL_STATUSES = new Set<TaskStatus>(["completed", "failed", "interrupted"]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function asNullableString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function asNullableNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function parseTask(value: unknown): Task | null {
  if (!isRecord(value) || typeof value.taskId !== "string") return null;
  const type = value.type === "pull" ? "pull" : value.type === "scan" ? "scan" : null;
  const status = typeof value.status === "string" ? value.status : null;
  if (!type || !status || !["pending", "running", "waiting_for_decision", "completed", "failed", "interrupted"].includes(status)) return null;
  const conflictMode = value.conflictMode === "ask" ? "ask" : "policy";
  return {
    taskId: value.taskId,
    type,
    status: status as TaskStatus,
    conflictMode,
    requestId: asNullableString(value.requestId),
    operationId: typeof value.operationId === "string" ? value.operationId : "",
    progressCurrent: asNullableNumber(value.progressCurrent),
    progressTotal: asNullableNumber(value.progressTotal),
    lastRepo: asNullableString(value.lastRepo),
    lastResult: asNullableString(value.lastResult),
    error: asNullableString(value.error),
    resultJson: asNullableString(value.resultJson),
    createdAt: typeof value.createdAt === "string" ? value.createdAt : "",
    updatedAt: typeof value.updatedAt === "string" ? value.updatedAt : "",
  };
}

function parseEvent(value: unknown): TaskEvent | null {
  if (!isRecord(value) || typeof value.taskId !== "string") return null;
  return {
    taskId: value.taskId,
    repoId: asNullableNumber(value.repoId),
    repoStatus: asNullableString(value.repoStatus),
    conflictReason: asNullableString(value.conflictReason),
    conflict: parseConflictDetail(value.conflict),
    requestedAction: asNullableString(value.requestedAction),
    backupPath: asNullableString(value.backupPath),
    event: typeof value.event === "string" ? value.event : undefined,
    current: asNullableNumber(value.current),
    total: asNullableNumber(value.total),
    lastRepo: asNullableString(value.lastRepo),
    lastResult: asNullableString(value.lastResult),
    error: asNullableString(value.error),
    durationMs: asNullableNumber(value.durationMs),
  };
}

function parseConflict(value: unknown): TaskConflict | null {
  if (!isRecord(value) || typeof value.taskId !== "string") return null;
  const repoId = asNullableNumber(value.repoId);
  const status = value.status;
  if (
    repoId === null ||
    !Number.isInteger(repoId) ||
    typeof value.repoName !== "string" ||
    typeof value.repoPath !== "string" ||
    !["waiting_decision", "resolving", "failed", "interrupted"].includes(String(status))
  ) {
    return null;
  }
  const allowedActions = Array.isArray(value.allowedActions)
    ? value.allowedActions.filter(
        (action): action is "backup" | "overwrite" | "abort" =>
          action === "backup" || action === "overwrite" || action === "abort",
      )
    : [];
  const requestedAction =
    value.requestedAction === "backup" ||
    value.requestedAction === "overwrite" ||
    value.requestedAction === "abort"
      ? value.requestedAction
      : null;
  return {
    taskId: value.taskId,
    repoId,
    repoName: value.repoName,
    repoPath: value.repoPath,
    status: status as TaskConflict["status"],
    conflictReason: asNullableString(value.conflictReason),
    conflictDetail: parseConflictDetail(value.conflict),
    allowedActions,
    requestedAction,
    result: asNullableString(value.result),
    durationMs: asNullableNumber(value.durationMs),
    backupPath: asNullableString(value.backupPath),
    error: asNullableString(value.error),
  };
}

function friendlyFetchMessage(message: string): string {
  if (message === "Failed to fetch" || message.includes("Failed to fetch")) {
    return "无法连接本地服务 (Failed to fetch)，请确认 xingshu-server 运行于 12681";
  }
  return message;
}

async function responseError(response: Response): Promise<Error> {
  try {
    const body: unknown = await response.json();
    if (isRecord(body) && typeof body.detail === "string") return new Error(friendlyFetchMessage(body.detail));
    if (isRecord(body) && typeof body.message === "string") return new Error(friendlyFetchMessage(body.message));
  } catch {
    // Fall through to the status when a proxy returns a non-JSON error page.
  }
  const raw = `HTTP ${response.status}`;
  return new Error(friendlyFetchMessage(raw));
}

export function useTaskStore() {
  const task = ref<Task | null>(null);
  const taskConflicts = ref<TaskConflict[]>([]);
  const connection = ref<TaskConnection>("idle");
  const error = ref("");
  const progressPercent = computed(() => {
    const current = task.value?.progressCurrent;
    const total = task.value?.progressTotal;
    const status = task.value?.status;
    if (status === "completed") {
      if (total === null || total === undefined || total <= 0) return 100;
    }
    if (current === null || current === undefined || total === null || total === undefined || total <= 0) return null;
    return Math.min(100, Math.max(0, (current / total) * 100));
  });
  const isBusy = computed(() => task.value !== null && !TERMINAL_STATUSES.has(task.value.status));
  const connectionLabel = computed(() => ({
    idle: "未连接",
    connecting: "正在连接",
    connected: "实时连接",
    reconnecting: "正在重连",
    closed: "连接已关闭",
    error: "连接异常",
  })[connection.value]);

  let eventSource: EventSource | null = null;
  let reconnectTimer: number | undefined;
  let reconnectAttempt = 0;
  let activeTaskId: string | null = null;
  let disposed = false;

  function clearReconnectTimer(): void {
    if (reconnectTimer !== undefined) {
      window.clearTimeout(reconnectTimer);
      reconnectTimer = undefined;
    }
  }

  function closeEventSource(): void {
    eventSource?.close();
    eventSource = null;
  }

  function stopConnection(nextState: TaskConnection = "closed"): void {
    clearReconnectTimer();
    closeEventSource();
    connection.value = nextState;
  }

  async function getTask(taskId: string): Promise<Task> {
    let response: Response;
    try {
      response = await fetch(`/api/v1/tasks/${encodeURIComponent(taskId)}`);
    } catch (reason) {
      throw new Error(friendlyFetchMessage(reason instanceof Error ? reason.message : String(reason)));
    }
    if (!response.ok) throw await responseError(response);
    const parsed = parseTask(await response.json() as unknown);
    if (!parsed) throw new Error("任务响应格式无效");
    return parsed;
  }

  async function loadConflicts(taskId: string): Promise<void> {
    let response: Response;
    try {
      response = await fetch(`/api/v1/tasks/${encodeURIComponent(taskId)}/conflicts`);
    } catch (reason) {
      throw new Error(friendlyFetchMessage(reason instanceof Error ? reason.message : String(reason)));
    }
    if (!response.ok) throw await responseError(response);
    const value: unknown = await response.json();
    if (!Array.isArray(value)) throw new Error("冲突响应格式无效");
    const parsed = value.map(parseConflict).filter((item): item is TaskConflict => item !== null);
    if (activeTaskId === taskId) taskConflicts.value = parsed;
  }

  async function refresh(taskId: string): Promise<Task> {
    let next: Task;
    try {
      next = await getTask(taskId);
    } catch (reason) {
      throw new Error(friendlyFetchMessage(reason instanceof Error ? reason.message : String(reason)));
    }
    if (activeTaskId !== taskId) return next;
    task.value = next;
    if (next.type === "pull") {
      await loadConflicts(taskId);
    } else {
      taskConflicts.value = [];
    }
    if (TERMINAL_STATUSES.has(next.status)) stopConnection("closed");
    return next;
  }

  function applyEvent(message: MessageEvent<string>): void {
    let value: unknown;
    try {
      value = JSON.parse(message.data) as unknown;
    } catch {
      error.value = "任务事件格式无效";
      return;
    }
    const next = parseEvent(value);
    if (!next || next.taskId !== activeTaskId || !task.value) return;
    const eventName = next.event ?? message.type;
    task.value = {
      ...task.value,
      status: eventName === "completed"
        ? "completed"
        : eventName === "failed"
          ? "failed"
          : eventName === "waiting_for_decision"
            ? "waiting_for_decision"
            : "running",
      progressCurrent: next.current ?? task.value.progressCurrent,
      progressTotal: next.total ?? task.value.progressTotal,
      lastRepo: next.lastRepo ?? task.value.lastRepo,
      lastResult: next.lastResult ?? task.value.lastResult,
      error: next.error ?? task.value.error,
      updatedAt: new Date().toISOString(),
    };
    if (eventName === "conflict" || eventName === "decision_started" || eventName === "decision_applied" || eventName === "decision_failed") {
      void loadConflicts(next.taskId).catch((reason) => {
        error.value = reason instanceof Error ? reason.message : "无法读取冲突状态";
      });
    }
    if (eventName === "completed" || eventName === "failed") {
      stopConnection("closed");
      void refresh(next.taskId).catch((reason) => {
        error.value = reason instanceof Error ? reason.message : "无法读取任务终态";
      });
    }
  }

  function applySnapshot(message: MessageEvent<string>): void {
    let value: unknown;
    try {
      value = JSON.parse(message.data) as unknown;
    } catch {
      error.value = "任务快照格式无效";
      return;
    }
    const next = parseTask(value);
    if (!next || next.taskId !== activeTaskId) return;
    task.value = next;
    if (next.type === "pull") {
      void loadConflicts(next.taskId).catch((reason) => {
        error.value = reason instanceof Error ? reason.message : "无法读取冲突状态";
      });
    } else {
      taskConflicts.value = [];
    }
    if (TERMINAL_STATUSES.has(next.status)) stopConnection("closed");
  }

  function scheduleReconnect(taskId: string): void {
    clearReconnectTimer();
    if (disposed || activeTaskId !== taskId || !isBusy.value) return;
    reconnectAttempt += 1;
    const delay = Math.min(5000, 500 * 2 ** Math.min(reconnectAttempt - 1, 3));
    reconnectTimer = window.setTimeout(() => {
      reconnectTimer = undefined;
      connect(taskId);
    }, delay);
  }

  async function fallbackAndReconnect(taskId: string): Promise<void> {
    try {
      const next = await refresh(taskId);
      if (TERMINAL_STATUSES.has(next.status)) return;
    } catch (reason) {
      error.value = reason instanceof Error ? reason.message : "无法读取任务状态";
      connection.value = "error";
    }
    scheduleReconnect(taskId);
  }

  function connect(taskId: string): void {
    if (disposed || activeTaskId !== taskId || !isBusy.value) return;
    closeEventSource();
    connection.value = reconnectAttempt > 0 ? "reconnecting" : "connecting";
    const source = new EventSource(`/api/v1/tasks/${encodeURIComponent(taskId)}/stream`);
    eventSource = source;
    source.onopen = () => {
      if (eventSource !== source || activeTaskId !== taskId) return;
      reconnectAttempt = 0;
      connection.value = "connected";
    };
    source.onerror = () => {
      if (eventSource !== source || activeTaskId !== taskId) return;
      closeEventSource();
      if (!isBusy.value) {
        connection.value = "closed";
        return;
      }
      connection.value = "reconnecting";
      void fallbackAndReconnect(taskId);
    };
    source.addEventListener("snapshot", (event: Event) => applySnapshot(event as MessageEvent<string>));
    for (const eventName of [
      "started",
      "progress",
      "conflict",
      "waiting_for_decision",
      "decision_started",
      "decision_applied",
      "decision_failed",
      "repo_completed",
      "completed",
      "failed",
    ]) {
      source.addEventListener(eventName, (event: Event) => applyEvent(event as MessageEvent<string>));
    }
  }

  async function start(type: TaskType): Promise<void> {
    stopConnection("idle");
    task.value = null;
    taskConflicts.value = [];
    activeTaskId = null;
    error.value = "";
    let response: Response;
    try {
      response = await fetch("/api/v1/tasks", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ type, ...(type === "pull" ? { conflictMode: "ask" } : {}) }),
      });
    } catch (reason) {
      throw new Error(friendlyFetchMessage(reason instanceof Error ? reason.message : String(reason)));
    }
    if (!response.ok) throw await responseError(response);
    const created: unknown = await response.json();
    if (!isRecord(created) || typeof created.taskId !== "string") throw new Error("任务创建响应格式无效");
    activeTaskId = created.taskId;
    const initial = parseTask({
      ...created,
      type,
      conflictMode: type === "pull" ? "ask" : "policy",
      requestId: null,
      progressCurrent: 0,
      progressTotal: null,
      lastRepo: null,
      lastResult: null,
      error: null,
      resultJson: null,
      createdAt: "",
      updatedAt: "",
    });
    if (!initial) throw new Error("任务创建响应格式无效");
    task.value = initial;
    try {
      await refresh(activeTaskId);
    } catch (reason) {
      error.value = reason instanceof Error ? reason.message : "无法读取任务状态";
    }
    if (isBusy.value) connect(activeTaskId);
  }

  async function decide(
    repoId: number,
    action: "backup" | "overwrite" | "abort",
  ): Promise<void> {
    if (!activeTaskId) return;
    let response: Response;
    try {
      response = await fetch(
        `/api/v1/tasks/${encodeURIComponent(activeTaskId)}/repos/${repoId}/decision`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ action }),
        },
      );
    } catch (reason) {
      throw new Error(friendlyFetchMessage(reason instanceof Error ? reason.message : String(reason)));
    }
    if (!response.ok) throw await responseError(response);
    await loadConflicts(activeTaskId);
  }

  function dismiss(): void {
    activeTaskId = null;
    stopConnection("idle");
    task.value = null;
    taskConflicts.value = [];
    error.value = "";
  }

  onBeforeUnmount(() => {
    disposed = true;
    stopConnection("idle");
  });

  return {
    task,
    taskConflicts,
    connection,
    connectionLabel,
    error,
    progressPercent,
    isBusy,
    start,
    refresh,
    loadConflicts,
    decide,
    dismiss,
  };
}
