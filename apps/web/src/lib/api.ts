export type Camera = {
  id: string;
  name: string;
  brand: string;
  serial: string;
  local_port: number;
  auto_start: boolean;
};

export type CameraInput = {
  name: string;
  brand: string;
  serial: string;
  username: string;
  password: string;
  port: number;
  local_port: number;
  auto_start: boolean;
};

export type Health = {
  api_version: string;
  status: "ok" | "degraded";
  storage: "legacy_json" | "sqlite";
  camera_count: number;
};

export type Recording = {
  id: string;
  camera_id: string;
  camera_name: string;
  started_at: string;
  ended_at: string | null;
  kind: string;
  bytes: number;
  status: string;
  archive_available: boolean;
};

export type PlaybackTicket = { url: string; expires_in_seconds: number };
export type LiveTicket = { protocol: "hls"; url: string; expires_in_seconds: number };

export type Provider = {
  id: string;
  name: string;
  main_server: string;
};

export type ProviderInput = {
  name: string;
  main_server: string;
  app_username: string;
  app_userkey: string;
};

export type Tunnel = { id: string; status: string };
export type ApiToken = { id: string; name: string; expires_at: string | null; enabled: boolean };
export type UserSummary = { username: string; role: string; created_at: string };
export type CameraDiagnostics = {
  camera_id: string;
  provider: string;
  provider_configured: boolean;
  tunnel_status: "stopped" | "starting" | "running" | "error" | string;
  tunnel_error: string | null;
  local_port: number;
  rtsp_path: string;
  next_action: string;
};
export type EventSummary = {
  id: string;
  kind: string;
  camera_id: string;
  camera_name: string;
  occurred_at: string;
  severity: string;
  message: string;
  source: string;
};

export type AuditEvent = {
  id: string;
  actor: string;
  action: string;
  path: string;
  status: number;
  created_at: string;
};

export type CurrentUser = {
  username: string;
  role: string;
  auth_type: "session" | "token";
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

  const response = await fetch(path, { ...init, headers, credentials: "include" });
  const body = (await response.json().catch(() => undefined)) as ApiErrorBody | T | undefined;
  if (!response.ok) {
    const errorBody = body as ApiErrorBody | undefined;
    throw new ApiError(errorBody?.message ?? errorBody?.error ?? response.statusText, response.status, errorBody);
  }
  return body as T;
}

export const api = {
  login: (username: string, password: string) =>
    request<{ token: string }>("/api/v1/auth/login", {
      method: "POST",
      body: JSON.stringify({ username, password }),
    }),
  refresh: () => request<{ token: string }>("/api/v1/auth/refresh", { method: "POST" }),
  logout: () => request<void>("/api/v1/auth/logout", { method: "POST" }),
  me: () => request<CurrentUser>("/api/v1/me"),
  cameras: () => request<Camera[]>("/api/v1/cameras"),
  createCamera: (input: CameraInput) => request<Camera>("/api/v1/cameras", { method: "POST", body: JSON.stringify(input) }),
  updateCamera: (id: string, input: Partial<CameraInput>) => request<Camera>(`/api/v1/cameras/${id}`, { method: "PATCH", body: JSON.stringify(input) }),
  deleteCamera: (id: string) => request<void>(`/api/v1/cameras/${id}`, { method: "DELETE" }),
  health: () => request<Health>("/api/v1/health"),
  recordings: (filters: { camera_id?: string; status?: string } = {}) => {
    const params = new URLSearchParams();
    if (filters.camera_id) params.set("camera_id", filters.camera_id);
    if (filters.status) params.set("status", filters.status);
    const query = params.toString();
    return request<Recording[]>(`/api/v1/recordings${query ? `?${query}` : ""}`);
  },
  playbackTicket: (id: string) => request<PlaybackTicket>(`/api/v1/recordings/${id}/playback-ticket`, { method: "POST" }),
  archiveRecording: (id: string) => request<Recording>(`/api/v1/recordings/${id}/archive`, { method: "POST" }),
  providers: () => request<Provider[]>("/api/v1/providers"),
  createProvider: (input: ProviderInput) => request<Provider>("/api/v1/providers", { method: "POST", body: JSON.stringify(input) }),
  updateProvider: (id: string, input: Partial<ProviderInput>) => request<Provider>(`/api/v1/providers/${id}`, { method: "PATCH", body: JSON.stringify(input) }),
  deleteProvider: (id: string) => request<void>(`/api/v1/providers/${id}`, { method: "DELETE" }),
  tunnels: () => request<Tunnel[]>("/api/v1/tunnels"),
  cameraDiagnostics: (id: string) => request<CameraDiagnostics>(`/api/v1/cameras/${id}/diagnostics`),
  liveTicket: (id: string) => request<LiveTicket>(`/api/v1/cameras/${id}/live-ticket`, { method: "POST" }),
  events: () => request<EventSummary[]>("/api/v1/events"),
  startCamera: (id: string) => request<void>(`/api/v1/cameras/${id}/start`, { method: "POST" }),
  stopCamera: (id: string) => request<void>(`/api/v1/cameras/${id}/stop`, { method: "POST" }),
  tokens: () => request<ApiToken[]>("/api/v1/tokens"),
  createToken: (input: { name: string; expires_at: string | null; enabled: boolean }) => request<ApiToken & { token: string }>("/api/v1/tokens", { method: "POST", body: JSON.stringify(input) }),
  updateToken: (id: string, input: Partial<Pick<ApiToken, "name" | "expires_at" | "enabled">>) => request<ApiToken>(`/api/v1/tokens/${id}`, { method: "PATCH", body: JSON.stringify(input) }),
  deleteToken: (id: string) => request<void>(`/api/v1/tokens/${id}`, { method: "DELETE" }),
  users: () => request<UserSummary[]>("/api/v1/users"),
  createUser: (input: { username: string; password: string; role: string }) => request<UserSummary>("/api/v1/users", { method: "POST", body: JSON.stringify(input) }),
  audit: () => request<AuditEvent[]>("/api/v1/audit"),
};
