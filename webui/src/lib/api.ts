export function friendlyError(message: string): string {
  if (message === "Failed to fetch" || message.includes("Failed to fetch")) {
    return "无法连接本地服务 (Failed to fetch)，请确认 xingshu-server 运行于 12681";
  }
  if (message.startsWith("HTTP 409")) return "任务冲突：已有同类任务正在运行";
  if (message.startsWith("HTTP 404")) return "未找到资源 (404)，请刷新后重试";
  return message;
}

export async function request(url: string, options?: RequestInit): Promise<Response> {
  const response = await fetch(url, options);
  if (!response.ok && response.status !== 409) throw new Error(`HTTP ${response.status}`);
  return response;
}
