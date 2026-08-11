export type Camera = {
  id: string;
  name: string;
  brand: string;
  serial: string;
  local_port: number;
  auto_start: boolean;
};

export type Health = {
  api_version: string;
  status: "ok" | "degraded";
  storage: "legacy_json" | "sqlite";
  camera_count: number;
};

export type ApiErrorBody = {
  code?: string;
  message?: string;
  error?: string;
};

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly body?: ApiErrorBody,
  ) {
    super(message);
  }
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const token = window.localStorage.getItem("camrelay_access_token");
  const headers = new Headers(init.headers);
  headers.set("Accept", "application/json");
  if (init.body && !headers.has("Content-Type")) headers.set("Content-Type", "application/json");
  if (token) headers.set("Authorization", `Bearer ${token}`);

  const response = await fetch(path, { ...init, headers });
  const body = (await response.json().catch(() => undefined)) as ApiErrorBody | T | undefined;
  if (!response.ok) {
    const errorBody = body as ApiErrorBody | undefined;
    throw new ApiError(errorBody?.message ?? errorBody?.error ?? response.statusText, response.status, errorBody);
  }
  return body as T;
}

export const api = {
  login: (username: string, password: string) =>
    request<{ token: string }>("/api/login", {
      method: "POST",
      body: JSON.stringify({ username, password }),
    }),
  cameras: () => request<Camera[]>("/api/v1/cameras"),
  health: () => request<Health>("/api/v1/health"),
};
