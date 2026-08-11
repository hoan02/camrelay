import { createContext, useContext, useEffect, useState } from "react";
import { Navigate, NavLink, Outlet, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ApiError, Camera, CameraInput, ProviderInput } from "../lib/api";
import { Locale, translate } from "../lib/i18n";

type IconName = "grid" | "camera" | "archive" | "settings" | "info" | "plus" | "arrow" | "sun" | "moon" | "logout" | "menu";

function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  const paths: Record<IconName, JSX.Element> = {
    grid: <><rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /><rect x="14" y="14" width="7" height="7" rx="1" /></>,
    camera: <><path d="M4 8.5A2.5 2.5 0 0 1 6.5 6H9l1.4-2h3.2L15 6h2.5A2.5 2.5 0 0 1 20 8.5v7A2.5 2.5 0 0 1 17.5 18h-11A2.5 2.5 0 0 1 4 15.5z" /><circle cx="12" cy="12" r="3" /></>,
    archive: <><path d="M4 7h16v13H4z" /><path d="M3 4h18v3H3zM9 11h6" /></>,
    settings: <><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.8 1.8 0 0 0 .4 2l.1.1-1.8 1.8-.1-.1a1.8 1.8 0 0 0-2-.4 1.8 1.8 0 0 0-1.1 1.7v.2h-2.6v-.2a1.8 1.8 0 0 0-1.1-1.7 1.8 1.8 0 0 0-2 .4l-.1.1-1.8-1.8.1-.1a1.8 1.8 0 0 0 .4-2 1.8 1.8 0 0 0-1.7-1.1H6v-2.6h.2a1.8 1.8 0 0 0 1.7-1.1 1.8 1.8 0 0 0-.4-2l-.1-.1 1.8-1.8.1.1a1.8 1.8 0 0 0 2 .4A1.8 1.8 0 0 0 12.4 5v-.2H15V5a1.8 1.8 0 0 0 1.1 1.7 1.8 1.8 0 0 0 2-.4l.1-.1L20 8l-.1.1a1.8 1.8 0 0 0-.4 2 1.8 1.8 0 0 0 1.7 1.1h.2v2.6h-.2a1.8 1.8 0 0 0-1.8 1.2Z" /></>,
    info: <><circle cx="12" cy="12" r="9" /><path d="M12 11v5M12 8h.01" /></>,
    plus: <><path d="M12 5v14M5 12h14" /></>,
    arrow: <><path d="M5 12h13M13 7l5 5-5 5" /></>,
    sun: <><circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" /></>,
    moon: <path d="M20.5 15.2A8.5 8.5 0 0 1 8.8 3.5 8.6 8.6 0 1 0 20.5 15.2Z" />,
    logout: <><path d="M10 5H5v14h5M14 8l4 4-4 4M18 12H9" /></>,
    menu: <><path d="M4 7h16M4 12h16M4 17h16" /></>,
  };
  return <svg aria-hidden="true" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">{paths[name]}</svg>;
}

const navItems: Array<{ to: string; label: string; icon: IconName }> = [
  { to: "/dashboard", label: "nav.dashboard", icon: "grid" },
  { to: "/cameras", label: "nav.cameras", icon: "camera" },
  { to: "/providers", label: "nav.providers", icon: "settings" },
  { to: "/recordings", label: "nav.recordings", icon: "archive" },
];

type Theme = "dark" | "light";
const LocaleContext = createContext<{ locale: Locale; setLocale: (locale: Locale) => void; t: (key: string) => string } | null>(null);
const ThemeContext = createContext<{ theme: Theme; setTheme: (theme: Theme) => void; toggleTheme: () => void } | null>(null);
function useLocale() { const value = useContext(LocaleContext); if (!value) throw new Error("Locale context is missing"); return value; }
function useTheme() { const value = useContext(ThemeContext); if (!value) throw new Error("Theme context is missing"); return value; }

export function App() {
  const [locale, setLocaleState] = useState<Locale>(() => window.localStorage.getItem("camrelay_locale") === "vi" ? "vi" : "en");
  const [theme, setTheme] = useState<Theme>(() => window.localStorage.getItem("camrelay_theme") === "light" ? "light" : "dark");
  const setLocale = (next: Locale) => { setLocaleState(next); window.localStorage.setItem("camrelay_locale", next); };
  const toggleTheme = () => setTheme(current => current === "dark" ? "light" : "dark");
  useEffect(() => { document.documentElement.dataset.theme = theme; window.localStorage.setItem("camrelay_theme", theme); }, [theme]);
  const t = (key: string) => translate(locale, key);
  return <LocaleContext.Provider value={{ locale, setLocale, t }}><ThemeContext.Provider value={{ theme, setTheme, toggleTheme }}><Routes>
    <Route path="/login" element={<Login />} />
    <Route element={<ProtectedLayout />}>
      <Route index element={<Navigate to="/dashboard" replace />} />
      <Route path="dashboard" element={<Dashboard />} />
      <Route path="cameras" element={<Cameras />} />
      <Route path="providers" element={<Providers />} />
      <Route path="recordings" element={<Recordings />} />
      <Route path="settings" element={<Settings />} />
      <Route path="about" element={<Placeholder title="About Camrelay" eyebrow="SYSTEM NOTES" description="Technical architecture, security boundaries, and provider capability notes will be documented here." icon="info" />} />
    </Route>
    <Route path="*" element={<Navigate to="/dashboard" replace />} />
  </Routes></ThemeContext.Provider></LocaleContext.Provider>;
}

function ProtectedLayout() {
  const navigate = useNavigate();
  const location = useLocation();
  const [mobileOpen, setMobileOpen] = useState(false);
  const { t } = useLocale();
  const { theme, toggleTheme } = useTheme();
  const health = useQuery({ queryKey: ["health"], queryFn: api.health, refetchInterval: 30_000 });
  const hasAuthHint = Boolean(window.localStorage.getItem("camrelay_authenticated") || window.localStorage.getItem("camrelay_access_token"));
  const me = useQuery({ queryKey: ["me"], queryFn: api.me, enabled: hasAuthHint, retry: false, staleTime: 60_000 });

  useEffect(() => {
    if (me.isError && me.error instanceof ApiError && me.error.status === 401) {
      window.localStorage.removeItem("camrelay_authenticated");
      window.localStorage.removeItem("camrelay_access_token");
      navigate("/login", { replace: true, state: { from: location.pathname } });
    }
  }, [location.pathname, me.error, me.isError, navigate]);

  if (!window.localStorage.getItem("camrelay_authenticated") && !window.localStorage.getItem("camrelay_access_token")) return <Navigate to="/login" replace state={{ from: location.pathname }} />;

  const logout = async () => { await api.logout().catch(() => undefined); window.localStorage.removeItem("camrelay_authenticated"); window.localStorage.removeItem("camrelay_access_token"); navigate("/login"); };
  return <div className="app-shell">
    <aside className={`sidebar ${mobileOpen ? "sidebar-open" : ""}`}>
      <div className="brand"><span className="brand-mark"><i /><i /><i /></span><span>camrelay</span></div>
      <div className="sidebar-kicker">{t("sidebar.kicker")}</div>
      <nav className="nav-list" aria-label="Primary navigation">
        {navItems.map(item => <NavLink onClick={() => setMobileOpen(false)} className={({ isActive }) => `nav-link ${isActive ? "active" : ""}`} to={item.to} key={item.to}><Icon name={item.icon} /><span>{t(item.label)}</span></NavLink>)}
      </nav>
      <div className="sidebar-bottom">
        <NavLink className={({ isActive }) => `nav-link ${isActive ? "active" : ""}`} to="/settings"><Icon name="settings" /><span>{t("nav.settings")}</span></NavLink>
        <NavLink className={({ isActive }) => `nav-link ${isActive ? "active" : ""}`} to="/about"><Icon name="info" /><span>{t("nav.about")}</span></NavLink>
        <div className="profile-card"><span className="avatar">{(me.data?.username ?? "C").slice(0, 1).toUpperCase()}</span><span className="profile-copy"><b>{me.data?.username ?? t("profile.admin")}</b><small>{me.data?.role ?? t("profile.local")}</small></span><button className="icon-button subtle" onClick={logout} title={t("action.signout")}><Icon name="logout" size={16} /></button></div>
      </div>
    </aside>
    {mobileOpen && <button className="scrim" onClick={() => setMobileOpen(false)} aria-label="Close menu" />}
    <main className="main-area">
      <header className="topbar"><button className="icon-button mobile-menu" onClick={() => setMobileOpen(true)} aria-label="Open menu"><Icon name="menu" /></button><div className="crumb"><span>Camrelay</span><b>/</b><strong>{location.pathname.slice(1) || "dashboard"}</strong></div><div className="top-actions"><span className={`runtime-pill ${health.isError || health.data?.status === "degraded" ? "runtime-warning" : ""}`}><i /> {health.isLoading ? t("runtime.checking") : health.isError ? t("runtime.unavailable") : t("runtime.ready")}</span><button className="icon-button" onClick={toggleTheme} title={theme === "dark" ? t("settings.theme.light") : t("settings.theme.dark")}><Icon name={theme === "dark" ? "sun" : "moon"} size={17} /></button></div></header>
      <div className="page-content"><Outlet /></div>
    </main>
  </div>;
}

function Login() {
  const navigate = useNavigate();
  const location = useLocation();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (event: React.FormEvent) => {
    event.preventDefault(); setError(""); setBusy(true);
    try { await api.login(username, password); window.localStorage.setItem("camrelay_authenticated", "1"); navigate((location.state as { from?: string } | null)?.from ?? "/dashboard", { replace: true }); }
    catch (cause) { setError(cause instanceof ApiError ? cause.message : "Camrelay API is not reachable."); }
    finally { setBusy(false); }
  };
  return <div className="login-page"><div className="login-orbit orbit-a" /><div className="login-orbit orbit-b" /><div className="login-panel"><div className="brand login-brand"><span className="brand-mark"><i /><i /><i /></span><span>camrelay</span></div><div className="login-intro"><span className="eyebrow">PRIVATE CAMERA GATEWAY</span><h1>Bring every view<br /><em>back home.</em></h1><p>A calm control plane for remote cameras, local RTSP, and your own archive.</p></div><form onSubmit={submit} className="login-form"><label>Username<input autoComplete="username" value={username} onChange={event => setUsername(event.target.value)} placeholder="Enter username" required /></label><label>Password<input autoComplete="current-password" type="password" value={password} onChange={event => setPassword(event.target.value)} placeholder="Enter password" required /></label>{error && <p className="form-error">{error}</p>}<button className="primary-button" disabled={busy}>{busy ? "Connecting…" : "Enter console"}<Icon name="arrow" size={17} /></button></form><small className="login-note">Credentials stay on your Camrelay server.</small></div><div className="login-caption"><span>SELF-HOSTED / ENCRYPTED / YOURS</span><span>v1 foundation</span></div></div>;
}

function Dashboard() {
  const { data: cameras, isLoading, isError } = useQuery({ queryKey: ["cameras"], queryFn: api.cameras });
  const count = cameras?.length ?? 0;
  return <><PageHeading eyebrow="OVERVIEW" title="Good to see you." description="One quiet place to watch the relay, cameras, and archive." action={<NavLink className="primary-button compact" to="/cameras"><Icon name="plus" size={16} />Add camera</NavLink>} /><section className="metric-grid"><Metric label="Cameras" value={isLoading ? "—" : String(count).padStart(2, "0")} detail={isError ? "API needs attention" : "Configured devices"} tone="signal" icon="camera" /><Metric label="Relay sessions" value="—" detail="Waiting for live telemetry" icon="grid" /><Metric label="Archive" value="—" detail="Recording index is migrating" icon="archive" /><Metric label="System" value="OK" detail="Console foundation" tone="amber" icon="settings" /></section><section className="dashboard-grid"><div className="panel signal-panel"><div className="panel-heading"><div><span className="eyebrow">RELAY SIGNAL</span><h2>Live topology</h2></div><span className="status-dot"><i />Ready</span></div><div className="topology"><div className="node"><span className="node-icon"><Icon name="camera" /></span><b>Remote cameras</b><small>Dahua / compatible P2P</small></div><div className="topology-line"><i /><i /><i /></div><div className="node emphasized"><span className="node-icon"><span className="brand-mark tiny"><i /><i /><i /></span></span><b>Camrelay core</b><small>Rust relay service</small></div><div className="topology-line muted"><i /><i /><i /></div><div className="node"><span className="node-icon"><Icon name="archive" /></span><b>Local media</b><small>RTSP / archive</small></div></div><div className="panel-foot"><span>{isError ? "Connect the API to load live camera telemetry." : "Live telemetry will appear as cameras are connected."}</span><NavLink to="/about" className="text-link">Read architecture <Icon name="arrow" size={15} /></NavLink></div></div><div className="panel activity-panel"><div className="panel-heading"><div><span className="eyebrow">RECENT ACTIVITY</span><h2>Nothing noisy.</h2></div><Icon name="info" size={17} /></div><div className="empty-state"><span className="empty-icon"><Icon name="archive" /></span><p>No events yet</p><small>Connection and recording events will surface here.</small></div></div></section></>;
}

function Cameras() {
  const { data: cameras, isLoading, isError } = useQuery({ queryKey: ["cameras"], queryFn: api.cameras });
  const { data: tunnels } = useQuery({ queryKey: ["tunnels"], queryFn: api.tunnels, refetchInterval: 5_000 });
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const mutation = useMutation({ mutationFn: api.createCamera, onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["cameras"] }); setOpen(false); } });
  const lifecycle = useMutation({
    mutationFn: ({ id, running }: { id: string; running: boolean }) => running ? api.stopCamera(id) : api.startCamera(id),
    onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["tunnels"] }); },
  });
  const remove = useMutation({
    mutationFn: api.deleteCamera,
    onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["cameras"] }); await queryClient.invalidateQueries({ queryKey: ["tunnels"] }); },
  });
  const statusFor = (id: string) => tunnels?.find(tunnel => tunnel.id === id)?.status ?? "stopped";
  const removeCamera = (id: string) => { if (window.confirm("Delete this camera configuration?")) remove.mutate(id); };
  return <><PageHeading eyebrow="DEVICE FLEET" title="Cameras" description="Manage remote devices and the local relay endpoints that represent them." action={<button className="primary-button compact" onClick={() => setOpen(true)}><Icon name="plus" size={16} />Add camera</button>} />{isLoading && <div className="panel loading-panel">Loading camera inventory…</div>}{isError && <div className="panel empty-panel"><span className="empty-icon"><Icon name="camera" /></span><h2>Camera API is not connected</h2><p>The new console is ready for the versioned API. The legacy service can continue running while this boundary is migrated.</p></div>}{cameras && cameras.length === 0 && <div className="panel empty-panel"><span className="empty-icon"><Icon name="camera" /></span><h2>Your fleet is empty</h2><p>Add the first camera when the provider and credential migration is ready.</p><button className="secondary-button" onClick={() => setOpen(true)}><Icon name="plus" size={15} />Add first camera</button></div>}{cameras && cameras.length > 0 && <div className="camera-grid">{cameras.map(camera => <CameraCard key={camera.id} camera={camera} status={statusFor(camera.id)} busy={lifecycle.isPending || remove.isPending} onToggle={() => lifecycle.mutate({ id: camera.id, running: statusFor(camera.id) === "running" })} onDelete={() => removeCamera(camera.id)} />)}</div>}{(lifecycle.error || remove.error) && <p className="form-error">{(lifecycle.error instanceof ApiError ? lifecycle.error.message : remove.error instanceof ApiError ? remove.error.message : "Camera action failed.")}</p>}{open && <CameraDialog busy={mutation.isPending} error={mutation.error instanceof ApiError ? mutation.error.message : mutation.error ? "Could not save camera." : ""} onClose={() => { setOpen(false); mutation.reset(); }} onSubmit={(input) => mutation.mutate(input)} />}</>;
}

function CameraDialog({ busy, error, onClose, onSubmit }: { busy: boolean; error: string; onClose: () => void; onSubmit: (input: CameraInput) => void }) {
  const [form, setForm] = useState<CameraInput>({ name: "", brand: "", serial: "", username: "admin", password: "", port: 554, local_port: 8551, auto_start: false });
  const update = <K extends keyof CameraInput>(key: K, value: CameraInput[K]) => setForm(current => ({ ...current, [key]: value }));
  return <div className="dialog-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) onClose(); }}><section className="dialog" role="dialog" aria-modal="true" aria-labelledby="camera-dialog-title"><div className="dialog-heading"><div><span className="eyebrow">NEW DEVICE</span><h2 id="camera-dialog-title">Add camera</h2><p>Credentials are sent to Camrelay and never returned by the API.</p></div><button className="icon-button" onClick={onClose} aria-label="Close dialog">×</button></div><div className="form-grid"><label>Name<input value={form.name} onChange={event => update("name", event.target.value)} placeholder="Front door" autoFocus /></label><label>Provider<input value={form.brand} onChange={event => update("brand", event.target.value)} placeholder="Dahua" /></label><label>Serial<input value={form.serial} onChange={event => update("serial", event.target.value)} placeholder="Device serial" /></label><label>RTSP username<input value={form.username} onChange={event => update("username", event.target.value)} /></label><label>RTSP password<input type="password" value={form.password} onChange={event => update("password", event.target.value)} /></label><label>Remote port<input type="number" min="1" value={form.port} onChange={event => update("port", Number(event.target.value))} /></label><label>Local port<input type="number" min="1" value={form.local_port} onChange={event => update("local_port", Number(event.target.value))} /></label><label className="check-field"><input type="checkbox" checked={form.auto_start} onChange={event => update("auto_start", event.target.checked)} />Start when Camrelay boots</label></div>{error && <p className="form-error dialog-error">{error}</p>}<div className="dialog-actions"><button className="secondary-button" onClick={onClose}>Cancel</button><button className="primary-button" disabled={busy} onClick={() => onSubmit(form)}>{busy ? "Saving…" : "Save camera"}</button></div></section></div>;
}

function Recordings() {
  const { data: recordings, isLoading, isError } = useQuery({ queryKey: ["recordings"], queryFn: api.recordings });
  return <><PageHeading eyebrow="MEDIA LIBRARY" title="Recordings" description="Closed segments, local retention, and the path to your cloud archive." />{isLoading && <div className="panel loading-panel">Loading recording index…</div>}{isError && <div className="panel empty-panel"><span className="empty-icon"><Icon name="archive" /></span><h2>Recording API is not connected</h2><p>The new media surface is wired to the v1 contract and will remain read-only until archive controls migrate.</p></div>}{recordings && recordings.length === 0 && <div className="panel empty-panel"><span className="empty-icon"><Icon name="archive" /></span><h2>No closed segments</h2><p>Once recording is enabled and a segment closes, it will appear here with its retention and archive state.</p></div>}{recordings && recordings.length > 0 && <div className="panel recordings-list"><div className="recordings-header"><span>SEGMENT</span><span>CAMERA</span><span>STATUS</span><span>SIZE</span></div>{recordings.map(recording => <div className="recording-row" key={recording.id}><div><b>{formatDate(recording.started_at)}</b><small>{recording.kind}{recording.ended_at ? ` · ${formatTime(recording.ended_at)}` : " · in progress"}</small></div><span>{recording.camera_name}</span><span className={`recording-status ${recording.status}`}>{recording.archive_available ? "Archived" : recording.status}</span><span>{formatBytes(recording.bytes)}</span></div>)}</div>}</>;
}

function Providers() {
  const { data: providers, isLoading, isError } = useQuery({ queryKey: ["providers"], queryFn: api.providers });
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const mutation = useMutation({ mutationFn: api.createProvider, onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["providers"] }); setOpen(false); } });
  const remove = useMutation({
    mutationFn: api.deleteProvider,
    onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["providers"] }); },
  });
  return <><PageHeading eyebrow="P2P PROFILES" title="Providers" description="Signaling profiles used by the relay. Secrets stay server-side and compatibility is always device-tested." action={<button className="primary-button compact" onClick={() => setOpen(true)}><Icon name="plus" size={16} />Add provider</button>} />{isLoading && <div className="panel loading-panel">Loading provider profiles…</div>}{isError && <div className="panel empty-panel"><span className="empty-icon"><Icon name="settings" /></span><h2>Provider API is not connected</h2><p>Provider editing will be enabled after the secret-backed v1 write contract is migrated.</p></div>}{providers && providers.length === 0 && <div className="panel empty-panel"><span className="empty-icon"><Icon name="settings" /></span><h2>No provider profiles</h2><p>Add an authorized P2P platform profile before onboarding a camera.</p><button className="secondary-button" onClick={() => setOpen(true)}><Icon name="plus" size={15} />Add first provider</button></div>}{providers && providers.length > 0 && <div className="provider-grid">{providers.map(provider => <article className="provider-card" key={provider.id}><div className="provider-icon"><span className="brand-mark tiny"><i /><i /><i /></span></div><div className="provider-copy"><h2>{provider.name}</h2><p>{provider.main_server}</p></div><span className="provider-badge">Configured</span><button className="icon-button subtle provider-remove" title="Delete provider" onClick={() => { if (window.confirm("Delete this provider profile?")) remove.mutate(provider.id); }}>×</button><small className="provider-note">Platform profile only · compatibility not implied</small></article>)}</div>}{remove.error && <p className="form-error">{remove.error instanceof ApiError ? remove.error.message : "Provider action failed."}</p>}{open && <ProviderDialog busy={mutation.isPending} error={mutation.error instanceof ApiError ? mutation.error.message : mutation.error ? "Could not save provider." : ""} onClose={() => { setOpen(false); mutation.reset(); }} onSubmit={(input) => mutation.mutate(input)} />}</>;
}

function ProviderDialog({ busy, error, onClose, onSubmit }: { busy: boolean; error: string; onClose: () => void; onSubmit: (input: ProviderInput) => void }) {
  const [form, setForm] = useState<ProviderInput>({ name: "", main_server: "", app_username: "", app_userkey: "" });
  const update = <K extends keyof ProviderInput>(key: K, value: ProviderInput[K]) => setForm(current => ({ ...current, [key]: value }));
  return <div className="dialog-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) onClose(); }}><section className="dialog" role="dialog" aria-modal="true" aria-labelledby="provider-dialog-title"><div className="dialog-heading"><div><span className="eyebrow">NEW PROFILE</span><h2 id="provider-dialog-title">Add provider</h2><p>Platform credentials are encrypted on the server and never shown in summaries.</p></div><button className="icon-button" onClick={onClose} aria-label="Close dialog">×</button></div><div className="form-grid"><label>Name<input value={form.name} onChange={event => update("name", event.target.value)} placeholder="Dahua / KBVision" autoFocus /></label><label>Signaling server<input value={form.main_server} onChange={event => update("main_server", event.target.value)} placeholder="host:8800" /></label><label>App username<input value={form.app_username} onChange={event => update("app_username", event.target.value)} /></label><label>App userkey<input type="password" value={form.app_userkey} onChange={event => update("app_userkey", event.target.value)} /></label></div>{error && <p className="form-error dialog-error">{error}</p>}<div className="dialog-actions"><button className="secondary-button" onClick={onClose}>Cancel</button><button className="primary-button" disabled={busy} onClick={() => onSubmit(form)}>{busy ? "Saving…" : "Save provider"}</button></div></section></div>;
}

function formatDate(value: string) { return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date(value)); }
function formatTime(value: string) { return new Intl.DateTimeFormat(undefined, { timeStyle: "short" }).format(new Date(value)); }
function formatBytes(value: number) { if (!value) return "—"; if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`; return `${(value / (1024 * 1024)).toFixed(1)} MB`; }

function CameraCard({ camera, status, busy, onToggle, onDelete }: { camera: Camera; status: string; busy: boolean; onToggle: () => void; onDelete: () => void }) { const running = status === "running"; return <article className="camera-card"><div className="camera-preview"><span className="preview-label">NO LIVE PREVIEW</span><span className="preview-grid" /><span className={`camera-live-badge ${running ? "is-running" : ""}`}><i />{status}</span></div><div className="camera-body"><div className="camera-title"><span className={`camera-status ${running ? "is-running" : ""}`} /><div><h3>{camera.name}</h3><small>{camera.brand} · {camera.serial}</small></div><button className="icon-button subtle" title="Delete camera" onClick={onDelete} disabled={busy}>×</button></div><div className="camera-meta"><span>RTSP :{camera.local_port}</span><span>{camera.auto_start ? "Auto-start" : "Manual start"}</span></div><div className="camera-actions"><button className="secondary-button compact" onClick={onToggle} disabled={busy}>{running ? "Stop relay" : "Start relay"}</button></div></div></article>; }

function PageHeading({ eyebrow, title, description, action }: { eyebrow: string; title: string; description: string; action?: React.ReactNode }) { return <div className="page-heading"><div><span className="eyebrow">{eyebrow}</span><h1>{title}</h1><p>{description}</p></div>{action}</div>; }
function Metric({ label, value, detail, icon, tone = "default" }: { label: string; value: string; detail: string; icon: IconName; tone?: string }) { return <div className={`metric-card ${tone}`}><div className="metric-icon"><Icon name={icon} size={17} /></div><span>{label}</span><strong>{value}</strong><small>{detail}</small></div>; }
function Settings() {
  const { locale, setLocale, t } = useLocale();
  const { theme, setTheme } = useTheme();
  return <><PageHeading eyebrow={t("page.settings.eyebrow")} title={t("page.settings.title")} description={t("page.settings.description")} /><div className="settings-grid"><section className="panel setting-card"><div className="setting-heading"><span className="setting-icon"><Icon name="info" /></span><div><h2>{t("settings.language.title")}</h2><p>{t("settings.language.description")}</p></div></div><div className="choice-group"><button className={locale === "en" ? "choice active" : "choice"} onClick={() => setLocale("en")}>{t("settings.language.english")}</button><button className={locale === "vi" ? "choice active" : "choice"} onClick={() => setLocale("vi")}>{t("settings.language.vietnamese")}</button></div></section><section className="panel setting-card"><div className="setting-heading"><span className="setting-icon"><Icon name={theme === "dark" ? "moon" : "sun"} /></span><div><h2>{t("settings.theme.title")}</h2><p>{t("settings.theme.description")}</p></div></div><div className="choice-group"><button className={theme === "dark" ? "choice active" : "choice"} onClick={() => setTheme("dark")}>{t("settings.theme.dark")}</button><button className={theme === "light" ? "choice active" : "choice"} onClick={() => setTheme("light")}>{t("settings.theme.light")}</button></div></section><section className="panel setting-card security-setting"><div className="setting-heading"><span className="setting-icon"><Icon name="settings" /></span><div><h2>{t("settings.security.title")}</h2><p>{t("settings.security.description")}</p></div></div><span className="security-mark"><i /> HttpOnly session</span></section></div></>;
}

function Placeholder({ title, eyebrow, description, icon }: { title: string; eyebrow: string; description: string; icon: IconName }) { return <><PageHeading eyebrow={eyebrow} title={title} description={description} /><div className="panel empty-panel"><span className="empty-icon"><Icon name={icon} /></span><h2>Workspace boundary ready</h2><p>This route is intentionally present now so deep links and future clients have a stable information architecture.</p></div></>; }
