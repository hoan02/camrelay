import { createContext, useContext, useEffect, useRef, useState } from "react";
import { Navigate, NavLink, Outlet, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ApiError, Camera, CameraDiagnostics, CameraInput, ProviderInput } from "../lib/api";
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
  { to: "/tokens", label: "nav.tokens", icon: "settings" },
  { to: "/users", label: "nav.users", icon: "settings" },
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
      <Route path="tokens" element={<Tokens />} />
      <Route path="users" element={<Users />} />
      <Route path="settings" element={<Settings />} />
      <Route path="about" element={<AboutPage />} />
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
  const queryClient = useQueryClient();
  const health = useQuery({ queryKey: ["health"], queryFn: api.health, refetchInterval: 30_000 });
  const hasAuthHint = Boolean(window.localStorage.getItem("camrelay_authenticated") || window.localStorage.getItem("camrelay_access_token"));
  const me = useQuery({ queryKey: ["me"], queryFn: api.me, enabled: hasAuthHint, retry: false, staleTime: 60_000 });

  useEffect(() => {
    if (!hasAuthHint) return;
    const stream = new EventSource("/api/v1/system/stream");
    const onSystem = (event: MessageEvent<string>) => {
      try {
        const payload = JSON.parse(event.data) as { api_version: string; status: "ok" | "degraded"; storage: "legacy_json" | "sqlite"; camera_count: number; tunnels?: Array<{ id: string; status: string }> };
        queryClient.setQueryData(["health"], { api_version: payload.api_version, status: payload.status, storage: payload.storage, camera_count: payload.camera_count });
        if (payload.tunnels) queryClient.setQueryData(["tunnels"], payload.tunnels);
      } catch {
        // The stream is advisory; regular query refresh remains the fallback.
      }
    };
    stream.addEventListener("system", onSystem as EventListener);
    return () => { stream.removeEventListener("system", onSystem as EventListener); stream.close(); };
  }, [hasAuthHint, queryClient]);

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
  const { t } = useLocale();
  const navigate = useNavigate();
  const location = useLocation();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (event: React.FormEvent) => {
    event.preventDefault(); setError(""); setBusy(true);
    try { await api.login(username, password); window.localStorage.setItem("camrelay_authenticated", "1"); navigate((location.state as { from?: string } | null)?.from ?? "/dashboard", { replace: true }); }
    catch (cause) { setError(cause instanceof ApiError ? cause.message : t("login.unreachable")); }
    finally { setBusy(false); }
  };
  return <div className="login-page"><div className="login-orbit orbit-a" /><div className="login-orbit orbit-b" /><div className="login-panel"><div className="brand login-brand"><span className="brand-mark"><i /><i /><i /></span><span>camrelay</span></div><div className="login-intro"><span className="eyebrow">{t("login.eyebrow")}</span><h1>{t("login.title")}<br /><em>{t("login.titleAccent")}</em></h1><p>{t("login.description")}</p></div><form onSubmit={submit} className="login-form"><label>{t("login.username")}<input autoComplete="username" value={username} onChange={event => setUsername(event.target.value)} placeholder={t("login.usernamePlaceholder")} required /></label><label>{t("login.password")}<input autoComplete="current-password" type="password" value={password} onChange={event => setPassword(event.target.value)} placeholder={t("login.passwordPlaceholder")} required /></label>{error && <p className="form-error">{error}</p>}<button className="primary-button" disabled={busy}>{busy ? t("login.connecting") : t("login.submit")}<Icon name="arrow" size={17} /></button></form><small className="login-note">{t("login.note")}</small></div><div className="login-caption"><span>SELF-HOSTED / ENCRYPTED / YOURS</span><span>v1 foundation</span></div></div>;
}

function Dashboard() {
  const { t } = useLocale();
  const { data: cameras, isLoading, isError } = useQuery({ queryKey: ["cameras"], queryFn: api.cameras });
  const count = cameras?.length ?? 0;
  return <>
    <PageHeading eyebrow="OVERVIEW" title="Good to see you." description="One quiet place to watch the relay, cameras, and archive." action={<NavLink className="primary-button compact" to="/cameras"><Icon name="plus" size={16} />{t("dashboard.addCamera")}</NavLink>} />
    <section className="metric-grid">
      <Metric label={t("dashboard.cameras")} value={isLoading ? "—" : String(count).padStart(2, "0")} detail={isError ? t("runtime.unavailable") : t("dashboard.configuredDevices")} tone="signal" icon="camera" />
      <Metric label={t("dashboard.relaySessions")} value="—" detail={t("dashboard.waitingTelemetry")} icon="grid" />
      <Metric label={t("dashboard.archive")} value="—" detail={t("dashboard.recordingMigrating")} icon="archive" />
      <Metric label={t("dashboard.system")} value="OK" detail={t("dashboard.consoleFoundation")} tone="amber" icon="settings" />
    </section>
    <section className="dashboard-grid">
      <div className="panel signal-panel"><div className="panel-heading"><div><span className="eyebrow">RELAY SIGNAL</span><h2>{t("dashboard.liveTopology")}</h2></div><span className="status-dot"><i />{t("dashboard.ready")}</span></div><div className="topology"><div className="node"><span className="node-icon"><Icon name="camera" /></span><b>{t("dashboard.remoteCameras")}</b><small>{t("dashboard.dahuaCompatible")}</small></div><div className="topology-line"><i /><i /><i /></div><div className="node emphasized"><span className="node-icon"><span className="brand-mark tiny"><i /><i /><i /></span></span><b>{t("dashboard.camrelayCore")}</b><small>{t("dashboard.rustRelay")}</small></div><div className="topology-line muted"><i /><i /><i /></div><div className="node"><span className="node-icon"><Icon name="archive" /></span><b>{t("dashboard.localMedia")}</b><small>{t("dashboard.rtspArchive")}</small></div></div><div className="panel-foot"><span>{isError ? t("dashboard.connectApi") : t("dashboard.telemetryWillAppear")}</span><NavLink to="/about" className="text-link">{t("dashboard.readArchitecture")} <Icon name="arrow" size={15} /></NavLink></div></div>
      <div className="panel activity-panel"><div className="panel-heading"><div><span className="eyebrow">{t("dashboard.recentActivity")}</span><h2>{t("dashboard.nothingNoisy")}</h2></div><Icon name="info" size={17} /></div><div className="empty-state"><span className="empty-icon"><Icon name="archive" /></span><p>{t("dashboard.noEvents")}</p><small>{t("dashboard.eventsAppear")}</small></div></div>
    </section>
  </>;
}

function Cameras() {
  const { t } = useLocale();
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
  const removeCamera = (id: string) => { if (window.confirm(t("cameras.deleteConfirm"))) remove.mutate(id); };
  return <><PageHeading eyebrow="DEVICE FLEET" title="Cameras" description="Manage remote devices and the local relay endpoints that represent them." action={<button className="primary-button compact" onClick={() => setOpen(true)}><Icon name="plus" size={16} />{t("common.addCamera")}</button>} />{isLoading && <div className="panel loading-panel">{t("cameras.loading")}</div>}{isError && <div className="panel empty-panel"><span className="empty-icon"><Icon name="camera" /></span><h2>{t("cameras.apiUnavailable")}</h2><p>{t("cameras.apiUnavailableDescription")}</p></div>}{cameras && cameras.length === 0 && <div className="panel empty-panel"><span className="empty-icon"><Icon name="camera" /></span><h2>{t("cameras.emptyTitle")}</h2><p>{t("cameras.emptyDescription")}</p><button className="secondary-button" onClick={() => setOpen(true)}><Icon name="plus" size={15} />{t("common.addFirstCamera")}</button></div>}{cameras && cameras.length > 0 && <div className="camera-grid">{cameras.map(camera => <CameraCard key={camera.id} camera={camera} status={statusFor(camera.id)} busy={lifecycle.isPending || remove.isPending} onToggle={() => lifecycle.mutate({ id: camera.id, running: statusFor(camera.id) === "running" })} onDelete={() => removeCamera(camera.id)} />)}</div>}{(lifecycle.error || remove.error) && <p className="form-error">{(lifecycle.error instanceof ApiError ? lifecycle.error.message : remove.error instanceof ApiError ? remove.error.message : t("cameras.actionFailed"))}</p>}{open && <CameraDialog busy={mutation.isPending} error={mutation.error instanceof ApiError ? mutation.error.message : mutation.error ? t("cameras.saveFailed") : ""} onClose={() => { setOpen(false); mutation.reset(); }} onSubmit={(input) => mutation.mutate(input)} />}</>;
}

function CameraDialog({ busy, error, onClose, onSubmit }: { busy: boolean; error: string; onClose: () => void; onSubmit: (input: CameraInput) => void }) {
  const { t } = useLocale();
  const [form, setForm] = useState<CameraInput>({ name: "", brand: "", serial: "", username: "admin", password: "", port: 554, local_port: 8551, auto_start: false });
  const update = <K extends keyof CameraInput>(key: K, value: CameraInput[K]) => setForm(current => ({ ...current, [key]: value }));
  return <div className="dialog-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) onClose(); }}><section className="dialog" role="dialog" aria-modal="true" aria-labelledby="camera-dialog-title"><div className="dialog-heading"><div><span className="eyebrow">{t("cameraDialog.eyebrow")}</span><h2 id="camera-dialog-title">{t("cameraDialog.title")}</h2><p>{t("cameraDialog.description")}</p></div><button className="icon-button" onClick={onClose} aria-label={t("common.close")}>×</button></div><div className="form-grid"><label>{t("cameraDialog.name")}<input value={form.name} onChange={event => update("name", event.target.value)} placeholder={t("cameraDialog.namePlaceholder")} autoFocus /></label><label>{t("cameraDialog.provider")}<input value={form.brand} onChange={event => update("brand", event.target.value)} placeholder={t("cameraDialog.providerPlaceholder")} /></label><label>{t("cameraDialog.serial")}<input value={form.serial} onChange={event => update("serial", event.target.value)} placeholder={t("cameraDialog.serialPlaceholder")} /></label><label>{t("cameraDialog.rtspUsername")}<input value={form.username} onChange={event => update("username", event.target.value)} /></label><label>{t("cameraDialog.rtspPassword")}<input type="password" value={form.password} onChange={event => update("password", event.target.value)} /></label><label>{t("cameraDialog.remotePort")}<input type="number" min="1" value={form.port} onChange={event => update("port", Number(event.target.value))} /></label><label>{t("cameraDialog.localPort")}<input type="number" min="1" value={form.local_port} onChange={event => update("local_port", Number(event.target.value))} /></label><label className="check-field"><input type="checkbox" checked={form.auto_start} onChange={event => update("auto_start", event.target.checked)} />{t("cameraDialog.startOnBoot")}</label></div>{error && <p className="form-error dialog-error">{error}</p>}<div className="dialog-actions"><button className="secondary-button" onClick={onClose}>{t("common.cancel")}</button><button className="primary-button" disabled={busy} onClick={() => onSubmit(form)}>{busy ? t("common.saving") : t("cameraDialog.save")}</button></div></section></div>;
}

function Recordings() {
  const { t } = useLocale();
  const { data: recordings, isLoading, isError } = useQuery({ queryKey: ["recordings"], queryFn: api.recordings });
  const queryClient = useQueryClient();
  const [playing, setPlaying] = useState<{ id: string; url: string } | null>(null);
  const playback = useMutation({ mutationFn: api.playbackTicket, onSuccess: (ticket, id) => setPlaying({ id, url: ticket.url }) });
  const archive = useMutation({ mutationFn: api.archiveRecording, onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["recordings"] }); } });
  return <><PageHeading eyebrow="MEDIA LIBRARY" title="Recordings" description="Closed segments, local retention, and the path to your cloud archive." />{isLoading && <div className="panel loading-panel">{t("recordings.loading")}</div>}{isError && <div className="panel empty-panel"><span className="empty-icon"><Icon name="archive" /></span><h2>{t("recordings.apiUnavailable")}</h2><p>{t("recordings.apiUnavailableDescription")}</p></div>}{playing && <section className="panel playback-panel"><div className="panel-heading"><div><span className="eyebrow">{t("recordings.playbackTicket")}</span><h2>{t("recordings.privatePreview")}</h2></div><button className="icon-button" onClick={() => setPlaying(null)} aria-label={t("common.close")}>×</button></div><video className="recording-player" controls autoPlay src={playing.url} /></section>}{recordings && recordings.length === 0 && <div className="panel empty-panel"><span className="empty-icon"><Icon name="archive" /></span><h2>{t("recordings.noSegments")}</h2><p>{t("recordings.noSegmentsDescription")}</p></div>}{recordings && recordings.length > 0 && <div className="panel recordings-list"><div className="recordings-header"><span>{t("recordings.segment")}</span><span>{t("recordings.camera")}</span><span>{t("recordings.status")}</span><span>{t("recordings.size")}</span><span>{t("recordings.actions")}</span></div>{recordings.map(recording => <div className="recording-row" key={recording.id}><div><b>{formatDate(recording.started_at)}</b><small>{recording.kind}{recording.ended_at ? ` - ${formatTime(recording.ended_at)}` : ` - ${t("recordings.inProgress")}`}</small></div><span>{recording.camera_name}</span><span className={`recording-status ${recording.status}`}>{recording.archive_available ? t("recordings.archived") : recording.status}</span><span>{formatBytes(recording.bytes)}</span><div className="recording-actions"><button className="text-button" onClick={() => playback.mutate(recording.id)} disabled={playback.isPending}>{t("recordings.play")}</button>{!recording.archive_available && <button className="text-button" onClick={() => archive.mutate(recording.id)} disabled={archive.isPending}>{t("recordings.archive")}</button>}</div></div>)}</div>}{(playback.error || archive.error) && <p className="form-error">{(playback.error instanceof ApiError ? playback.error.message : archive.error instanceof ApiError ? archive.error.message : t("recordings.actionFailed"))}</p>}</>;
}

function Tokens() {
  const { t } = useLocale();
  const { data: tokens, isLoading, isError } = useQuery({ queryKey: ["tokens"], queryFn: api.tokens });
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [newSecret, setNewSecret] = useState("");
  const create = useMutation({
    mutationFn: () => api.createToken({ name, expires_at: null, enabled: true }),
    onSuccess: async result => { setNewSecret(result.token); setName(""); await queryClient.invalidateQueries({ queryKey: ["tokens"] }); },
  });
  const toggle = useMutation({ mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) => api.updateToken(id, { enabled }), onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["tokens"] }); } });
  const remove = useMutation({ mutationFn: api.deleteToken, onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["tokens"] }); } });
  return <><PageHeading eyebrow="ACCESS CONTROL" title="API tokens" description="Create scoped service credentials for Frigate, FFmpeg, and other trusted clients." /><section className="panel token-create"><div><span className="eyebrow">{t("tokens.newCredential")}</span><h2>{t("tokens.createTitle")}</h2><p>{t("tokens.createDescription")}</p></div><div className="token-create-form"><input value={name} onChange={event => setName(event.target.value)} placeholder={t("tokens.namePlaceholder")} aria-label={t("tokens.nameLabel")} /><button className="primary-button compact" onClick={() => create.mutate()} disabled={!name.trim() || create.isPending}>{t("tokens.create")}</button></div>{newSecret && <div className="token-secret"><span>{t("tokens.copyNow")}</span><code>{newSecret}</code><button className="text-button" onClick={() => navigator.clipboard?.writeText(newSecret)}>{t("tokens.copy")}</button></div>}{create.error && <p className="form-error">{create.error instanceof ApiError ? create.error.message : t("tokens.couldNotCreate")}</p>}</section>{isLoading && <div className="panel loading-panel">{t("tokens.loading")}</div>}{isError && <div className="panel empty-panel"><span className="empty-icon"><Icon name="settings" /></span><h2>{t("tokens.apiUnavailable")}</h2><p>{t("tokens.apiUnavailableDescription")}</p></div>}{tokens && tokens.length === 0 && <div className="panel empty-panel"><span className="empty-icon"><Icon name="settings" /></span><h2>{t("tokens.emptyTitle")}</h2><p>{t("tokens.emptyDescription")}</p></div>}{tokens && tokens.length > 0 && <div className="token-grid">{tokens.map(token => <article className="panel token-card" key={token.id}><div><h2>{token.name}</h2><small>{token.expires_at ? `${t("tokens.expires")} ${formatDate(token.expires_at)}` : t("tokens.noExpiry")}</small></div><span className={`token-state ${token.enabled ? "enabled" : "disabled"}`}><i />{token.enabled ? t("tokens.enabled") : t("tokens.disabled")}</span><div className="token-actions"><button className="text-button" onClick={() => toggle.mutate({ id: token.id, enabled: !token.enabled })} disabled={toggle.isPending}>{token.enabled ? t("tokens.disable") : t("tokens.enable")}</button><button className="text-button danger" onClick={() => { if (window.confirm(t("tokens.revokeConfirm"))) remove.mutate(token.id); }} disabled={remove.isPending}>{t("tokens.revoke")}</button></div></article>)}</div>}</>;
}

function Users() {
  const { t } = useLocale();
  const { data: users, isLoading, isError } = useQuery({ queryKey: ["users"], queryFn: api.users });
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const mutation = useMutation({ mutationFn: api.createUser, onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["users"] }); setOpen(false); } });
  return <><PageHeading eyebrow="ADMINISTRATION" title="Users" description="Manage appliance identities and roles without exposing password material." action={<button className="primary-button compact" onClick={() => setOpen(true)}><Icon name="plus" size={16} />{t("users.add")}</button>} />{isLoading && <div className="panel loading-panel">{t("users.loading")}</div>}{isError && <div className="panel empty-panel"><span className="empty-icon"><Icon name="settings" /></span><h2>{t("users.apiUnavailable")}</h2><p>{t("users.apiUnavailableDescription")}</p></div>}{users && users.length === 0 && <div className="panel empty-panel"><span className="empty-icon"><Icon name="settings" /></span><h2>{t("users.emptyTitle")}</h2><p>{t("users.emptyDescription")}</p><button className="secondary-button" onClick={() => setOpen(true)}><Icon name="plus" size={15} />{t("users.addFirst")}</button></div>}{users && users.length > 0 && <div className="user-grid">{users.map(user => <article className="panel user-card" key={user.username}><div className="avatar user-avatar">{user.username.slice(0, 1).toUpperCase()}</div><div><h2>{user.username}</h2><small>{t("users.created")} {formatDate(user.created_at)}</small></div><span className={`user-role ${user.role}`}>{user.role}</span></article>)}</div>}{open && <UserDialog busy={mutation.isPending} error={mutation.error instanceof ApiError ? mutation.error.message : mutation.error ? t("users.createFailed") : ""} onClose={() => { setOpen(false); mutation.reset(); }} onSubmit={input => mutation.mutate(input)} />}</>;
}

function UserDialog({ busy, error, onClose, onSubmit }: { busy: boolean; error: string; onClose: () => void; onSubmit: (input: { username: string; password: string; role: string }) => void }) {
  const { t } = useLocale();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState("viewer");
  return <div className="dialog-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) onClose(); }}><section className="dialog" role="dialog" aria-modal="true" aria-labelledby="user-dialog-title"><div className="dialog-heading"><div><span className="eyebrow">{t("page.users.eyebrow")}</span><h2 id="user-dialog-title">{t("users.createTitle")}</h2><p>{t("users.createDescription")}</p></div><button className="icon-button" onClick={onClose} aria-label={t("common.close")}>×</button></div><div className="form-grid"><label>{t("users.username")}<input value={username} onChange={event => setUsername(event.target.value)} placeholder={t("users.usernamePlaceholder")} autoFocus /></label><label>{t("users.password")}<input type="password" value={password} onChange={event => setPassword(event.target.value)} placeholder={t("users.passwordPlaceholder")} /></label><label>{t("users.role")}<select value={role} onChange={event => setRole(event.target.value)}><option value="admin">{t("users.roleAdmin")}</option><option value="operator">{t("users.roleOperator")}</option><option value="viewer">{t("users.roleViewer")}</option></select></label></div>{error && <p className="form-error dialog-error">{error}</p>}<div className="dialog-actions"><button className="secondary-button" onClick={onClose}>{t("common.cancel")}</button><button className="primary-button" disabled={busy || !username.trim() || password.length < 8} onClick={() => onSubmit({ username: username.trim(), password, role })}>{busy ? t("common.saving") : t("users.create")}</button></div></section></div>;
}

function Providers() {
  const { t } = useLocale();
  const { data: providers, isLoading, isError } = useQuery({ queryKey: ["providers"], queryFn: api.providers });
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const mutation = useMutation({ mutationFn: api.createProvider, onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["providers"] }); setOpen(false); } });
  const remove = useMutation({
    mutationFn: api.deleteProvider,
    onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["providers"] }); },
  });
  return <><PageHeading eyebrow="P2P PROFILES" title="Providers" description="Signaling profiles used by the relay. Secrets stay server-side and compatibility is always device-tested." action={<button className="primary-button compact" onClick={() => setOpen(true)}><Icon name="plus" size={16} />{t("common.addProvider")}</button>} />{isLoading && <div className="panel loading-panel">{t("providers.loading")}</div>}{isError && <div className="panel empty-panel"><span className="empty-icon"><Icon name="settings" /></span><h2>{t("providers.apiUnavailable")}</h2><p>{t("providers.apiUnavailableDescription")}</p></div>}{providers && providers.length === 0 && <div className="panel empty-panel"><span className="empty-icon"><Icon name="settings" /></span><h2>{t("providers.emptyTitle")}</h2><p>{t("providers.emptyDescription")}</p><button className="secondary-button" onClick={() => setOpen(true)}><Icon name="plus" size={15} />{t("common.addFirstProvider")}</button></div>}{providers && providers.length > 0 && <div className="provider-grid">{providers.map(provider => <article className="provider-card" key={provider.id}><div className="provider-icon"><span className="brand-mark tiny"><i /><i /><i /></span></div><div className="provider-copy"><h2>{provider.name}</h2><p>{provider.main_server}</p></div><span className="provider-badge">{t("providers.configured")}</span><button className="icon-button subtle provider-remove" title={t("common.delete")} onClick={() => { if (window.confirm(t("providers.deleteConfirm"))) remove.mutate(provider.id); }}>×</button><small className="provider-note">{t("providers.note")}</small></article>)}</div>}{remove.error && <p className="form-error">{remove.error instanceof ApiError ? remove.error.message : t("providers.actionFailed")}</p>}{open && <ProviderDialog busy={mutation.isPending} error={mutation.error instanceof ApiError ? mutation.error.message : mutation.error ? t("providers.saveFailed") : ""} onClose={() => { setOpen(false); mutation.reset(); }} onSubmit={(input) => mutation.mutate(input)} />}</>;
}

function ProviderDialog({ busy, error, onClose, onSubmit }: { busy: boolean; error: string; onClose: () => void; onSubmit: (input: ProviderInput) => void }) {
  const { t } = useLocale();
  const [form, setForm] = useState<ProviderInput>({ name: "", main_server: "", app_username: "", app_userkey: "" });
  const update = <K extends keyof ProviderInput>(key: K, value: ProviderInput[K]) => setForm(current => ({ ...current, [key]: value }));
  return <div className="dialog-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) onClose(); }}><section className="dialog" role="dialog" aria-modal="true" aria-labelledby="provider-dialog-title"><div className="dialog-heading"><div><span className="eyebrow">{t("providerDialog.eyebrow")}</span><h2 id="provider-dialog-title">{t("providerDialog.title")}</h2><p>{t("providerDialog.description")}</p></div><button className="icon-button" onClick={onClose} aria-label={t("common.close")}>×</button></div><div className="form-grid"><label>{t("providerDialog.name")}<input value={form.name} onChange={event => update("name", event.target.value)} placeholder={t("providerDialog.namePlaceholder")} autoFocus /></label><label>{t("providerDialog.signalingServer")}<input value={form.main_server} onChange={event => update("main_server", event.target.value)} placeholder={t("providerDialog.serverPlaceholder")} /></label><label>{t("providerDialog.appUsername")}<input value={form.app_username} onChange={event => update("app_username", event.target.value)} /></label><label>{t("providerDialog.appUserkey")}<input type="password" value={form.app_userkey} onChange={event => update("app_userkey", event.target.value)} /></label></div>{error && <p className="form-error dialog-error">{error}</p>}<div className="dialog-actions"><button className="secondary-button" onClick={onClose}>{t("common.cancel")}</button><button className="primary-button" disabled={busy} onClick={() => onSubmit(form)}>{busy ? t("common.saving") : t("providerDialog.save")}</button></div></section></div>;
}

function formatDate(value: string) { return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date(value)); }
function formatTime(value: string) { return new Intl.DateTimeFormat(undefined, { timeStyle: "short" }).format(new Date(value)); }
function formatBytes(value: number) { if (!value) return "—"; if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`; return `${(value / (1024 * 1024)).toFixed(1)} MB`; }

function CameraCard({ camera, status, busy, onToggle, onDelete }: { camera: Camera; status: string; busy: boolean; onToggle: () => void; onDelete: () => void }) {
  const { t } = useLocale();
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const [liveUrl, setLiveUrl] = useState<string | null>(null);
  const liveVideo = useRef<HTMLVideoElement>(null);
  const diagnostics = useQuery({
    queryKey: ["camera-diagnostics", camera.id],
    queryFn: () => api.cameraDiagnostics(camera.id),
    enabled: diagnosticsOpen,
  });
  const live = useMutation({ mutationFn: () => api.liveTicket(camera.id), onSuccess: ticket => setLiveUrl(ticket.url) });
  const running = status === "running";
  useEffect(() => { if (!running) setLiveUrl(null); }, [running]);
  useEffect(() => {
    const video = liveVideo.current;
    if (!video || !liveUrl) return;
    if (video.canPlayType("application/vnd.apple.mpegurl")) {
      video.src = liveUrl;
      return () => { video.removeAttribute("src"); video.load(); };
    }
    let disposed = false;
    let hls: { destroy: () => void; loadSource: (source: string) => void; attachMedia: (media: HTMLMediaElement) => void } | undefined;
    void import("hls.js").then(({ default: Hls }) => {
      if (disposed || !Hls.isSupported()) return;
      hls = new Hls({ enableWorker: true, lowLatencyMode: true });
      hls.loadSource(liveUrl);
      hls.attachMedia(video);
    });
    return () => {
      disposed = true;
      hls?.destroy();
    };
  }, [liveUrl]);
  return <>
    <article className="camera-card"><div className="camera-preview">{liveUrl ? <video ref={liveVideo} className="camera-live-player" controls autoPlay muted playsInline onError={() => setLiveUrl(null)} /> : <><span className="preview-label">{t("camera.noPreview")}</span><span className="preview-grid" /></>}<span className={`camera-live-badge ${running ? "is-running" : ""}`}><i />{status}</span></div><div className="camera-body"><div className="camera-title"><span className={`camera-status ${running ? "is-running" : ""}`} /><div><h3>{camera.name}</h3><small>{camera.brand} · {camera.serial}</small></div><button className="icon-button subtle" title={t("common.delete")} onClick={onDelete} disabled={busy}>×</button></div><div className="camera-meta"><span>RTSP :{camera.local_port}</span><span>{camera.auto_start ? t("common.autoStart") : t("common.manualStart")}</span></div><div className="camera-actions"><button className="secondary-button compact" onClick={onToggle} disabled={busy}>{running ? t("common.stopRelay") : t("common.startRelay")}</button><button className="primary-button compact" onClick={() => live.mutate()} disabled={!running || live.isPending}>{t("camera.live")}</button><button className="text-button" onClick={() => setDiagnosticsOpen(true)}>{t("camera.diagnostics")}</button></div>{live.error && <p className="form-error">{live.error instanceof ApiError ? live.error.message : t("camera.liveFailed")}</p>}</div></article>
    {diagnosticsOpen && <CameraDiagnosticsDialog diagnostics={diagnostics.data} loading={diagnostics.isLoading} error={diagnostics.isError} onClose={() => setDiagnosticsOpen(false)} />}
  </>;
}

function CameraDiagnosticsDialog({ diagnostics, loading, error, onClose }: { diagnostics?: CameraDiagnostics; loading: boolean; error: boolean; onClose: () => void }) {
  const { t } = useLocale();
  return <div className="dialog-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) onClose(); }}><section className="dialog" role="dialog" aria-modal="true" aria-labelledby="camera-diagnostics-title"><div className="dialog-heading"><div><span className="eyebrow">{t("camera.diagnostics")}</span><h2 id="camera-diagnostics-title">{diagnostics?.provider ?? "Camera"}</h2><p>{diagnostics?.next_action ?? (error ? t("camera.diagnosticsUnavailable") : t("common.loading"))}</p></div><button className="icon-button" onClick={onClose} aria-label={t("common.close")}>×</button></div>{loading && <div className="loading-panel">{t("common.loading")}</div>}{error && <p className="form-error">{t("camera.diagnosticsUnavailable")}</p>}{diagnostics && <div className="diagnostic-grid"><div><span>{t("camera.tunnelStatus")}</span><strong>{diagnostics.tunnel_status}</strong></div><div><span>{diagnostics.provider_configured ? t("camera.providerConfigured") : t("camera.providerMissing")}</span><strong>{diagnostics.provider}</strong></div><div><span>{t("camera.localPort")}</span><strong>{diagnostics.local_port}</strong></div><div><span>{t("camera.rtspPath")}</span><code>{diagnostics.rtsp_path}</code></div>{diagnostics.tunnel_error && <div className="diagnostic-wide"><span>{t("camera.tunnelError")}</span><strong>{diagnostics.tunnel_error}</strong></div>}<div className="diagnostic-wide"><span>{t("camera.nextAction")}</span><strong>{diagnostics.next_action}</strong></div></div>}</section></div>;
}

function PageHeading({ eyebrow, title, description, action }: { eyebrow: string; title: string; description: string; action?: React.ReactNode }) {
  const { locale } = useLocale();
  const localized = locale === "vi" ? ({
    "OVERVIEW|Good to see you.": ["TỔNG QUAN", "Mừng bạn trở lại.", "Một nơi yên tĩnh để theo dõi relay, camera và kho lưu trữ của bạn."],
    "DEVICE FLEET|Cameras": ["ĐỘI CAMERA", "Camera", "Quản lý thiết bị từ xa và các cổng relay cục bộ tương ứng."],
    "P2P PROFILES|Providers": ["HỒ SƠ P2P", "Nhà cung cấp", "Hồ sơ signaling của relay. Secret ở lại trên máy chủ; khả năng tương thích cần được kiểm thử thực tế."],
    "MEDIA LIBRARY|Recordings": ["THƯ VIỆN MEDIA", "Bản ghi", "Các phân đoạn đã đóng, thời hạn lưu cục bộ và lộ trình lên cloud."],
    "ACCESS CONTROL|API tokens": ["KIỂM SOÁT TRUY CẬP", "API token", "Tạo thông tin truy cập dịch vụ cho Frigate, FFmpeg và các client tin cậy khác."],
    "ADMINISTRATION|Users": ["QUẢN TRỊ", "Người dùng", "Quản lý identity và role của appliance mà không lộ password."],
  } as Record<string, [string, string, string]>)[`${eyebrow}|${title}`] : undefined;
  const copy = localized ?? [eyebrow, title, description];
  return <div className="page-heading"><div><span className="eyebrow">{copy[0]}</span><h1>{copy[1]}</h1><p>{copy[2]}</p></div>{action}</div>;
}
function Metric({ label, value, detail, icon, tone = "default" }: { label: string; value: string; detail: string; icon: IconName; tone?: string }) { return <div className={`metric-card ${tone}`}><div className="metric-icon"><Icon name={icon} size={17} /></div><span>{label}</span><strong>{value}</strong><small>{detail}</small></div>; }
function Settings() {
  const { locale, setLocale, t } = useLocale();
  const { theme, setTheme } = useTheme();
  return <><PageHeading eyebrow={t("page.settings.eyebrow")} title={t("page.settings.title")} description={t("page.settings.description")} /><div className="settings-grid"><section className="panel setting-card"><div className="setting-heading"><span className="setting-icon"><Icon name="info" /></span><div><h2>{t("settings.language.title")}</h2><p>{t("settings.language.description")}</p></div></div><div className="choice-group"><button className={locale === "en" ? "choice active" : "choice"} onClick={() => setLocale("en")}>{t("settings.language.english")}</button><button className={locale === "vi" ? "choice active" : "choice"} onClick={() => setLocale("vi")}>{t("settings.language.vietnamese")}</button></div></section><section className="panel setting-card"><div className="setting-heading"><span className="setting-icon"><Icon name={theme === "dark" ? "moon" : "sun"} /></span><div><h2>{t("settings.theme.title")}</h2><p>{t("settings.theme.description")}</p></div></div><div className="choice-group"><button className={theme === "dark" ? "choice active" : "choice"} onClick={() => setTheme("dark")}>{t("settings.theme.dark")}</button><button className={theme === "light" ? "choice active" : "choice"} onClick={() => setTheme("light")}>{t("settings.theme.light")}</button></div></section><section className="panel setting-card security-setting"><div className="setting-heading"><span className="setting-icon"><Icon name="settings" /></span><div><h2>{t("settings.security.title")}</h2><p>{t("settings.security.description")}</p></div></div><span className="security-mark"><i /> HttpOnly session</span></section></div></>;
}

function AboutPage() {
  const { t } = useLocale();
  return <>
    <PageHeading eyebrow={t("about.eyebrow")} title={t("about.title")} description={t("about.description")} />
    <div className="about-grid">
      <section className="panel about-card about-wide"><div className="panel-heading"><div><span className="eyebrow">{t("about.architectureTitle")}</span><h2>{t("about.architectureTitle")}</h2></div><Icon name="grid" size={17} /></div><p>{t("about.architectureDescription")}</p><div className="about-flow"><div><Icon name="camera" /><b>{t("about.remoteCamera")}</b><small>{t("about.p2p")}</small></div><span>→</span><div className="about-flow-accent"><Icon name="settings" /><b>{t("about.core")}</b><small>Rust / Axum / Tokio</small></div><span>→</span><div><Icon name="archive" /><b>{t("about.localRtsp")}</b><small>One port per camera</small></div><span>→</span><div><Icon name="grid" /><b>{t("about.clients")}</b><small>{t("about.media")}</small></div></div></section>
      <section className="panel about-card"><div className="panel-heading"><div><span className="eyebrow">{t("about.securityTitle")}</span><h2>{t("about.securityTitle")}</h2></div><Icon name="settings" size={17} /></div><p>{t("about.securityDescription")}</p><ul className="about-list"><li>{t("about.securityItemOne")}</li><li>{t("about.securityItemTwo")}</li><li>{t("about.securityItemThree")}</li><li>{t("about.securityItemFour")}</li></ul></section>
      <section className="panel about-card"><div className="panel-heading"><div><span className="eyebrow">{t("about.compatibilityTitle")}</span><h2>{t("about.compatibilityTitle")}</h2></div><Icon name="info" size={17} /></div><p>{t("about.compatibilityDescription")}</p><div className="about-callout"><strong>{t("about.imouTitle")}</strong><span>{t("about.imouDescription")}</span></div></section>
      <section className="panel about-card"><div className="panel-heading"><div><span className="eyebrow">{t("about.mediaTitle")}</span><h2>{t("about.mediaTitle")}</h2></div><Icon name="archive" size={17} /></div><p>{t("about.mediaDescription")}</p><div className="about-callout"><strong>{t("about.archiveTitle")}</strong><span>{t("about.archiveDescription")}</span></div></section>
      <section className="panel about-card"><div className="panel-heading"><div><span className="eyebrow">{t("about.contractTitle")}</span><h2>{t("about.contractTitle")}</h2></div><Icon name="arrow" size={17} /></div><p>{t("about.contractDescription")}</p><div className="about-code-lines"><code>POST /api/v1/auth/login</code><code>GET&nbsp; /api/v1/cameras</code><code>POST /api/v1/recordings/:id/playback-ticket</code></div></section>
      <section className="panel about-card about-wide"><div className="panel-heading"><div><span className="eyebrow">{t("about.roadmapTitle")}</span><h2>{t("about.roadmapTitle")}</h2></div><span className="status-dot"><i />v1 foundation</span></div><p>{t("about.roadmapDescription")}</p><div className="about-roadmap"><span className="done">Foundation</span><span className="active">Console</span><span>Live media</span><span>Operations</span><span>Mobile</span></div></section>
    </div>
  </>;
}
