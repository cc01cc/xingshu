import { computed, onBeforeUnmount, ref } from "vue";

export type TaskType = "scan" | "pull";
export type TaskStatus = "pending" | "running" | "completed" | "failed" | "interrupted";
export type TaskConnection = "idle" | "connecting" | "connected" | "reconnecting" | "closed" | "error";

export type Task = {
  taskId: string;
  type: TaskType;
  status: TaskStatus;
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

type TaskEvent = {
  taskId: string;
  event?: string;
  current?: number | null;
  total?: number | null;
  lastRepo?: string | null;
  lastResult?: string | null;
  error?: string | null;
  durationMs?: number | null;
};

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
  if (!type || !status || !["pending", "running", "completed", "failed", "interrupted"].includes(status)) return null;
  return {
    taskId: value.taskId,
    type,
    status: status as TaskStatus,
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
    event: typeof value.event === "string" ? value.event : undefined,
    current: asNullableNumber(value.current),
    total: asNullableNumber(value.total),
    lastRepo: asNullableString(value.lastRepo),
    lastResult: asNullableString(value.lastResult),
    error: asNullableString(value.error),
    durationMs: asNullableNumber(value.durationMs),
  };
}

async function responseError(response: Response): Promise<Error> {
  try {
    const body: unknown = await response.json();
    if (isRecord(body) && typeof body.detail === "string") return new Error(body.detail);
    if (isRecord(body) && typeof body.message === "string") return new Error(body.message);
  } catch {
    // Fall through to the status when a proxy returns a non-JSON error page.
  }
  return new Error(`HTTP ${response.status}`);
}

export function useTaskStore() {
  const task = ref<Task | null>(null);
  const connection = ref<TaskConnection>("idle");
  const error = ref("");
  const progressPercent = computed(() => {
    const current = task.value?.progressCurrent;
    const total = task.value?.progressTotal;
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
    const response = await fetch(`/api/v1/tasks/${encodeURIComponent(taskId)}`);
    if (!response.ok) throw await responseError(response);
    const parsed = parseTask(await response.json() as unknown);
    if (!parsed) throw new Error("任务响应格式无效");
    return parsed;
  }

  async function refresh(taskId: string): Promise<Task> {
    const next = await getTask(taskId);
    if (activeTaskId !== taskId) return next;
    task.value = next;
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
      status: eventName === "completed" ? "completed" : eventName === "failed" ? "failed" : "running",
      progressCurrent: next.current ?? task.value.progressCurrent,
      progressTotal: next.total ?? task.value.progressTotal,
      lastRepo: next.lastRepo ?? task.value.lastRepo,
      lastResult: next.lastResult ?? task.value.lastResult,
      error: next.error ?? task.value.error,
      updatedAt: new Date().toISOString(),
    };
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
    for (const eventName of ["started", "progress", "completed", "failed"]) {
      source.addEventListener(eventName, (event: Event) => applyEvent(event as MessageEvent<string>));
    }
  }

  async function start(type: TaskType): Promise<void> {
    stopConnection("idle");
    task.value = null;
    activeTaskId = null;
    error.value = "";
    const response = await fetch("/api/v1/tasks", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ type }),
    });
    if (!response.ok) throw await responseError(response);
    const created: unknown = await response.json();
    if (!isRecord(created) || typeof created.taskId !== "string") throw new Error("任务创建响应格式无效");
    activeTaskId = created.taskId;
    const initial = parseTask({
      ...created,
      type,
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

  function dismiss(): void {
    activeTaskId = null;
    stopConnection("idle");
    task.value = null;
    error.value = "";
  }

  onBeforeUnmount(() => {
    disposed = true;
    stopConnection("idle");
  });

  return { task, connection, connectionLabel, error, progressPercent, isBusy, start, refresh, dismiss };
}
