const state = {
  lang: localStorage.getItem('camrelay_language') || 'en',
  theme: localStorage.getItem('camrelay_theme') || (window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'light' : 'dark'),
  locale: {},
  token: localStorage.getItem('camrelay_token'),
  cameras: [],
  brands: [],
  tokens: [],
  recordings: [],
  recordingConfig: null,
  selectedRecordingId: null,
  tunnels: {},
  activeView: 'dashboard',
  pendingView: null,
  cameraStep: 1,
  editingCameraId: null,
  editingBrandId: null,
  editingTokenId: null,
  providerTests: new Set(),
  refreshTimer: null,
};

const builtinProviders = [
  { name: 'IMOU', status: 'experimental', descriptionKey: 'imouDescription' },
];

const $ = selector => document.querySelector(selector);
const $$ = selector => [...document.querySelectorAll(selector)];
const t = (key, vars = {}) => Object.entries(vars).reduce((value, [name, replacement]) => value.replace(`{${name}}`, replacement), state.locale[key] || key);
const escapeHtml = value => String(value ?? '').replace(/[&<>"']/g, character => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[character]));
const viewPaths = { dashboard: '/dashboard', cameras: '/cameras', recordings: '/recordings', providers: '/providers', tokens: '/tokens', settings: '/settings', about: '/about' };
const pathViews = Object.fromEntries(Object.entries(viewPaths).map(([view, path]) => [path, view]));

function viewFromLocation() {
  const path = window.location.pathname.replace(/\/+$/, '') || '/';
  return pathViews[path] || (path === '/login' ? null : 'dashboard');
}

function setRoute(view, replace = false) {
  const path = viewPaths[view] || viewPaths.dashboard;
  if (window.location.pathname !== path) {
    window.history[replace ? 'replaceState' : 'pushState']({ view }, '', path);
  }
}

function applyTheme(theme = state.theme) {
  state.theme = theme === 'light' ? 'light' : 'dark';
  document.documentElement.dataset.theme = state.theme;
  localStorage.setItem('camrelay_theme', state.theme);
  const toggle = $('#theme-toggle');
  if (toggle) {
    toggle.setAttribute('aria-checked', String(state.theme === 'dark'));
    toggle.setAttribute('aria-label', t(state.theme === 'dark' ? 'darkMode' : 'lightMode'));
    $('#theme-toggle-label').textContent = t(state.theme === 'dark' ? 'darkMode' : 'lightMode');
  }
}

async function loadLocale() {
  try {
    const response = await fetch(`/i18n/${state.lang}.json`);
    state.locale = response.ok ? await response.json() : {};
  } catch {
    state.locale = {};
  }
  document.documentElement.lang = state.lang;
  $$('[data-i18n]').forEach(element => { element.textContent = t(element.dataset.i18n); });
  $$('[data-i18n-aria-label]').forEach(element => { element.setAttribute('aria-label', t(element.dataset.i18nAriaLabel)); });
  $$('[data-i18n-placeholder]').forEach(element => { element.placeholder = t(element.dataset.i18nPlaceholder); });
  $('#language').value = state.lang;
  applyTheme(state.theme);
  const titles = { dashboard: 'overview', cameras: 'cameras', recordings: 'recordings', providers: 'providersTitle', tokens: 'apiTokensTitle', settings: 'settings', about: 'technicalGuide' };
  $('#page-title').textContent = t(titles[state.activeView] || 'overview');
}

function toast(message) {
  const element = $('#toast');
  element.textContent = message;
  element.classList.add('show');
  setTimeout(() => element.classList.remove('show'), 2400);
}

function setView(view, updateUrl = true) {
  state.activeView = view;
  if (updateUrl) setRoute(view);
  document.body.classList.remove('modal-open');
  $('#camera-wizard').classList.add('hidden');
  $('#provider-form').classList.add('hidden');
  $('#token-form').classList.add('hidden');
  $$('.nav-button').forEach(button => button.classList.toggle('active', button.dataset.view === view));
  $$('.view').forEach(section => section.classList.toggle('active', section.id === `view-${view}`));
  const titles = { dashboard: 'overview', cameras: 'cameras', recordings: 'recordings', providers: 'providersTitle', tokens: 'apiTokensTitle', settings: 'settings', about: 'technicalGuide' };
  $('#page-title').textContent = t(titles[view] || 'overview');
  if (view === 'dashboard') renderDashboard();
  if (view === 'cameras') renderCameras();
  if (view === 'recordings') { loadRecordings(); renderRecordings(); }
  if (view === 'providers') renderProviders();
  if (view === 'tokens') loadTokens();
}

function showLogin() {
  const currentView = viewFromLocation();
  if (currentView) state.pendingView = currentView;
  window.history.replaceState({ view: 'login' }, '', '/login');
  $('#login-screen').classList.remove('hidden');
  $('#app-shell').classList.add('hidden');
  clearInterval(state.refreshTimer);
}

function showApp() {
  if (window.location.pathname === '/login' || !pathViews[window.location.pathname.replace(/\/+$/, '')]) {
    state.activeView = state.pendingView || 'dashboard';
    state.pendingView = null;
    setRoute(state.activeView, true);
  }
  $('#login-screen').classList.add('hidden');
  $('#app-shell').classList.remove('hidden');
  setView(state.activeView, false);
  loadAll();
  state.refreshTimer = setInterval(loadTunnels, 5000);
}

function logout() {
  localStorage.removeItem('camrelay_token');
  state.token = null;
  showLogin();
}

async function api(path, options = {}) {
  try {
    const response = await fetch(`/api${path}`, {
      ...options,
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${state.token}`, ...(options.headers || {}) },
    });
    if (response.status === 401) logout();
    return response;
  } catch {
    toast(t('networkError'));
    return null;
  }
}

async function loadAll() {
  await Promise.all([loadCameras(), loadBrands(), loadTunnels(), loadRecordings()]);
  renderAll();
}

async function loadCameras() {
  const response = await api('/cameras');
  if (response?.ok) state.cameras = await response.json();
}

async function loadBrands() {
  const response = await api('/brands');
  if (response?.ok) state.brands = await response.json();
}

async function loadTokens() {
  const response = await api('/tokens');
  if (response?.ok) state.tokens = await response.json();
  renderTokens();
}

async function loadTunnels() {
  const response = await api('/tunnels');
  if (response?.ok) state.tunnels = Object.fromEntries((await response.json()).map(item => [item.id, item]));
  renderAll();
}

async function loadRecordings() {
  const [recordingsResponse, configResponse] = await Promise.all([api('/recordings?limit=200'), api('/recordings/config')]);
  if (recordingsResponse?.ok) state.recordings = await recordingsResponse.json();
  if (configResponse?.ok) state.recordingConfig = await configResponse.json();
  renderRecordings();
}

function getStatus(cameraId) {
  const status = state.tunnels[cameraId]?.status || 'stopped';
  if (status.startsWith('error')) return 'error';
  return status;
}

function statusBadge(status) {
  const normalized = status || 'stopped';
  return `<span class="status-badge status-${escapeHtml(normalized)}">${escapeHtml(t(normalized))}</span>`;
}

function renderAll() {
  renderDashboard();
  renderCameras();
  renderRecordings();
  renderProviders();
  if (state.activeView === 'tokens') renderTokens();
}

function renderDashboard() {
  const statuses = state.cameras.map(camera => getStatus(camera.id));
  $('#stat-total').textContent = state.cameras.length;
  $('#stat-running').textContent = statuses.filter(status => status === 'running').length;
  $('#stat-attention').textContent = statuses.filter(status => status === 'error').length;
  $('#stat-clients').textContent = '—';
  const container = $('#dashboard-cameras');
  if (!state.cameras.length) {
    container.innerHTML = `<div class="card empty-state"><div class="empty-icon">+</div><h3>${escapeHtml(t('noCameras'))}</h3><p>${escapeHtml(t('addFirstCamera'))}</p><button class="button-primary" data-action="add-camera">${escapeHtml(t('configureFirst'))}</button></div>`;
    return;
  }
  container.innerHTML = state.cameras.slice(0, 6).map(cameraCard).join('');
  bindDynamicActions(container);
}

function renderCameras() {
  const container = $('#camera-list');
  if (!state.cameras.length) {
    container.innerHTML = `<div class="card empty-state"><div class="empty-icon">+</div><h3>${escapeHtml(t('noCameras'))}</h3><p>${escapeHtml(t('addFirstCamera'))}</p><button class="button-primary" data-action="add-camera">${escapeHtml(t('configureFirst'))}</button></div>`;
    bindDynamicActions(container);
    return;
  }
  container.innerHTML = state.cameras.map(cameraCard).join('');
  bindDynamicActions(container);
}

function cameraCard(camera) {
  const tunnel = state.tunnels[camera.id];
  const status = getStatus(camera.id);
  const active = ['running', 'starting'].includes(status);
  const url = tunnel?.rtsp_url || '';
  return `<article class="card camera-card">
    <div class="camera-card-header"><div><div class="camera-idline">CAMERA / ${escapeHtml(camera.id.slice(0, 8).toUpperCase())}</div><h3 class="camera-title">${escapeHtml(camera.name)}</h3><div class="camera-provider">${escapeHtml(camera.brand)} · ${escapeHtml(camera.serial)}</div></div>${statusBadge(status)}</div>
    <div class="route-line"><span class="route-node">P2P</span><span class="route-connector"></span><span class="route-node">LOCAL RTSP</span><span class="route-port">:${escapeHtml(camera.local_port)}</span></div>
    <div class="camera-meta"><div><span class="meta-label">${escapeHtml(t('remoteRtspPort'))}</span><span class="meta-value">:${escapeHtml(camera.port)}</span></div><div><span class="meta-label">${escapeHtml(t('connectedClients'))}</span><span class="meta-value">—</span></div></div>
    <div class="camera-url" data-action="copy" data-value="${escapeHtml(url)}"><span class="meta-label">${escapeHtml(t('localEndpoint'))}</span>${escapeHtml(url || t('notAvailable'))}</div>
    <div class="card-actions"><button class="${active ? 'button-danger' : 'button-primary'} button-small" data-action="toggle-tunnel" data-id="${escapeHtml(camera.id)}"><span class="button-icon" aria-hidden="true">${active ? '■' : '▶'}</span>${escapeHtml(t(active ? 'stop' : 'start'))}</button><button class="button-secondary button-small" data-action="edit-camera" data-id="${escapeHtml(camera.id)}"><span class="button-icon" aria-hidden="true">✎</span>${escapeHtml(t('edit'))}</button><button class="button-ghost button-small" data-action="delete-camera" data-id="${escapeHtml(camera.id)}"><span class="button-icon" aria-hidden="true">⌫</span>${escapeHtml(t('delete'))}</button></div>
  </article>`;
}

function bindDynamicActions(container) {
  container.querySelectorAll('[data-action="add-camera"]').forEach(button => button.addEventListener('click', () => openCameraWizard()));
  container.querySelectorAll('[data-action="copy"]').forEach(element => element.addEventListener('click', () => element.dataset.value && copyText(element.dataset.value)));
  container.querySelectorAll('[data-action="toggle-tunnel"]').forEach(button => button.addEventListener('click', () => toggleTunnel(button.dataset.id)));
  container.querySelectorAll('[data-action="edit-camera"]').forEach(button => button.addEventListener('click', () => openCameraWizard(button.dataset.id)));
  container.querySelectorAll('[data-action="delete-camera"]').forEach(button => button.addEventListener('click', () => deleteCamera(button.dataset.id)));
}

function formatBytes(bytes) {
  if (!bytes) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return (bytes / Math.pow(1024, index)).toFixed(index ? 1 : 0) + ' ' + units[index];
}

function formatRecordingDate(value) {
  if (!value) return '—';
  return new Date(value).toLocaleString(state.lang === 'vi' ? 'vi-VN' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' });
}

function recordingStatusLabel(status) {
  return t({ local: 'localOnly', uploading: 'uploading', archived: 'archived', failed: 'failed' }[status] || status);
}

function renderRecordings() {
  const cameraFilter = $('#recording-camera-filter');
  const statusFilter = $('#recording-status-filter');
  const list = $('#recording-list');
  if (!cameraFilter || !statusFilter || !list) return;
  const selectedCamera = cameraFilter.value;
  cameraFilter.innerHTML = '<option value="">' + escapeHtml(t('allCameras')) + '</option>' + state.cameras.map(camera => '<option value="' + escapeHtml(camera.id) + '">' + escapeHtml(camera.name) + '</option>').join('');
  cameraFilter.value = state.cameras.some(camera => camera.id === selectedCamera) ? selectedCamera : '';
  const filtered = state.recordings.filter(record => (!cameraFilter.value || record.camera_id === cameraFilter.value) && (!statusFilter.value || record.status === statusFilter.value));
  $('#recording-count').textContent = filtered.length;
  if (!filtered.length) {
    list.innerHTML = '<div class="card empty-state"><div class="empty-icon">R</div><h3>' + escapeHtml(t('noRecordings')) + '</h3><p>' + escapeHtml(state.recordingConfig?.enabled ? t('noRecordingsYet') : t('recordingDisabled')) + '</p></div>';
    return;
  }
  list.innerHTML = filtered.map(record => {
    const canArchive = record.status === 'local' || record.status === 'failed';
    const error = record.error ? '<div class="recording-error">' + escapeHtml(record.error) + '</div>' : '';
    const archive = canArchive && state.recordingConfig?.archive_configured ? '<button class="button-secondary button-small" data-action="archive-recording" data-id="' + escapeHtml(record.id) + '"><span class="button-icon">↑</span>' + escapeHtml(t('archiveNow')) + '</button>' : '';
    return '<article class="card recording-card ' + (state.selectedRecordingId === record.id ? 'selected' : '') + '">' +
      '<div class="recording-card-heading"><div><span class="eyebrow">' + escapeHtml(record.camera_name) + '</span><h3>' + escapeHtml(formatRecordingDate(record.started_at)) + '</h3></div><span class="compatibility ' + escapeHtml(record.status) + '">' + escapeHtml(recordingStatusLabel(record.status)) + '</span></div>' +
      '<div class="recording-meta"><span>' + escapeHtml(formatBytes(record.bytes)) + '</span><span>' + escapeHtml(t(record.kind)) + '</span>' + (record.ended_at ? '<span>' + escapeHtml(t('endedAt')) + ': ' + escapeHtml(formatRecordingDate(record.ended_at)) + '</span>' : '') + '</div>' +
      error + '<div class="card-actions"><button class="button-primary button-small" data-action="play-recording" data-id="' + escapeHtml(record.id) + '"><span class="button-icon">▶</span>' + escapeHtml(t('play')) + '</button>' + archive + '</div></article>';
  }).join('');
  list.querySelectorAll('[data-action="play-recording"]').forEach(button => button.addEventListener('click', () => openRecording(button.dataset.id)));
  list.querySelectorAll('[data-action="archive-recording"]').forEach(button => button.addEventListener('click', () => archiveRecording(button.dataset.id)));
}

async function openRecording(id) {
  const record = state.recordings.find(item => item.id === id);
  if (!record) return;
  const response = await api('/recordings/' + id + '/playback-ticket', { method: 'POST' });
  if (!response?.ok) return;
  const ticket = await response.json();
  state.selectedRecordingId = id;
  $('#recording-player').classList.remove('hidden');
  $('#player-title').textContent = record.camera_name;
  $('#player-meta').textContent = formatRecordingDate(record.started_at) + ' · ' + formatBytes(record.bytes);
  const video = $('#recording-video');
  video.src = ticket.url;
  video.load();
  renderRecordings();
}

function closeRecordingPlayer() {
  state.selectedRecordingId = null;
  const video = $('#recording-video');
  video.pause();
  video.removeAttribute('src');
  video.load();
  $('#recording-player').classList.add('hidden');
  renderRecordings();
}

async function archiveRecording(id) {
  const response = await api('/recordings/' + id + '/archive', { method: 'POST' });
  if (!response?.ok) return;
  const saved = await response.json();
  state.recordings = state.recordings.map(record => record.id === id ? saved : record);
  renderRecordings();
  toast(t('archiveStarted'));
}

function renderProviders() {
  const configured = state.brands.map(brand => ({ ...brand, status: state.providerTests.has(brand.name.toLowerCase()) ? 'verified' : brand.name.toLowerCase() === 'imou' ? 'experimental' : 'needsTest', configured: true, descriptionKey: 'providerConfiguration' }));
  const names = new Set(configured.map(provider => provider.name.toLowerCase()));
  const providers = [...configured, ...builtinProviders.filter(provider => !names.has(provider.name.toLowerCase()))];
  $('#provider-list').innerHTML = providers.length ? providers.map(provider => `<article class="card provider-card"><div class="provider-header"><div class="provider-logo">${escapeHtml(provider.name.slice(0, 2).toUpperCase())}</div><span class="compatibility ${escapeHtml(provider.status)}">${escapeHtml(t(provider.status === 'needsTest' ? 'providerNeedsTest' : provider.status))}</span></div><h3>${escapeHtml(provider.name)}</h3><p>${escapeHtml(t(provider.descriptionKey))}</p><div class="muted">${escapeHtml(t('serverLabel'))}: <code>${escapeHtml(provider.main_server || t('notAvailable'))}</code></div>${provider.name.toLowerCase() === 'imou' ? `<div class="warning">${escapeHtml(t('imouWarning'))}</div>` : ''}<div class="card-actions"><button class="button-secondary button-small" data-action="provider-edit" data-id="${escapeHtml(provider.id || '')}" data-name="${escapeHtml(provider.name)}"><span class="button-icon" aria-hidden="true">✎</span>${escapeHtml(provider.configured ? t('edit') : t('providerConfiguration'))}</button>${provider.configured ? `<button class="button-ghost button-small" data-action="provider-delete" data-id="${escapeHtml(provider.id)}"><span class="button-icon" aria-hidden="true">⌫</span>${escapeHtml(t('delete'))}</button>` : ''}</div></article>`).join('') : `<div class="card empty-state"><h3>${escapeHtml(t('noProviders'))}</h3></div>`;
  $('#provider-list').querySelectorAll('[data-action="provider-edit"]').forEach(button => button.addEventListener('click', () => openProviderForm(button.dataset.id || null, button.dataset.name)));
  $('#provider-list').querySelectorAll('[data-action="provider-delete"]').forEach(button => button.addEventListener('click', () => deleteProvider(button.dataset.id)));
}

function renderTokens() {
  const body = $('#token-table-body');
  if (!state.tokens.length) { body.innerHTML = `<tr><td colspan="5" class="muted">${escapeHtml(t('noTokens'))}</td></tr>`; return; }
  body.innerHTML = state.tokens.map(item => { const expired = item.expires_at && Date.parse(item.expires_at) < Date.now(); const status = !item.enabled ? 'disabled' : expired ? 'expired' : 'active'; return `<tr><td>${escapeHtml(item.name)}</td><td><code>${escapeHtml(item.token.slice(0, 12))}••••••••</code></td><td>${item.expires_at ? new Date(item.expires_at).toLocaleString(state.lang === 'vi' ? 'vi-VN' : 'en-US') : '—'}</td><td><span class="status-badge status-${status === 'active' ? 'running' : status === 'expired' ? 'error' : 'stopped'}">${escapeHtml(t(status))}</span></td><td><div class="table-actions"><button class="button-secondary button-small" data-action="token-edit" data-id="${escapeHtml(item.id)}"><span class="button-icon" aria-hidden="true">✎</span>${escapeHtml(t('edit'))}</button><button class="button-danger button-small" data-action="token-delete" data-id="${escapeHtml(item.id)}"><span class="button-icon" aria-hidden="true">⌫</span>${escapeHtml(t('delete'))}</button></div></td></tr>`; }).join('');
  body.querySelectorAll('[data-action="token-edit"]').forEach(button => button.addEventListener('click', () => openTokenForm(button.dataset.id)));
  body.querySelectorAll('[data-action="token-delete"]').forEach(button => button.addEventListener('click', () => deleteToken(button.dataset.id)));
}

function openCameraWizard(id = null) {
  if (state.activeView !== 'cameras') setView('cameras');
  document.body.classList.add('modal-open');
  state.editingCameraId = id;
  state.cameraStep = 1;
  $('#camera-wizard').classList.remove('hidden');
  const camera = state.cameras.find(item => item.id === id);
  $('#wizard-title').textContent = t('setupCamera');
  $('#camera-name').value = camera?.name || '';
  $('#camera-serial').value = camera?.serial || '';
  $('#camera-username').value = camera?.username || 'admin';
  $('#camera-password').value = camera?.password || '';
  $('#remote-port').value = camera?.port || 554;
  $('#local-port').value = camera?.local_port || 8551 + state.cameras.length;
  $('#auto-start').checked = !!camera?.auto_start;
  renderProviderOptions(camera?.brand);
  renderWizardStep();
  $('#camera-wizard').scrollIntoView({ behavior: 'smooth', block: 'start' });
}

function renderProviderOptions(selected = '') {
  const options = state.brands.length ? state.brands : [];
  $('#provider-options').innerHTML = options.length ? options.map(provider => { const tested = state.providerTests.has(provider.name.toLowerCase()); return `<label class="provider-option ${provider.name === selected ? 'selected' : ''}"><input type="radio" name="camera-provider" value="${escapeHtml(provider.name)}" ${provider.name === selected ? 'checked' : ''}><span class="provider-option-title"><span class="provider-logo">${escapeHtml(provider.name.slice(0, 2).toUpperCase())}</span><strong>${escapeHtml(provider.name)}</strong><span class="compatibility ${tested ? 'verified' : 'needsTest'}">${escapeHtml(tested ? t('verified') : t('providerNeedsTest'))}</span></span><p>${escapeHtml(t('providerConfiguration'))}</p></label>`; }).join('') : `<div class="muted">${escapeHtml(t('noProviders'))} <button class="button-secondary button-small" data-action="add-provider-from-wizard">${escapeHtml(t('addProvider'))}</button></div>`;
  $('#provider-options').querySelectorAll('input').forEach(input => input.addEventListener('change', () => { $$('.provider-option').forEach(option => option.classList.toggle('selected', option.querySelector('input').checked)); }));
  $('#provider-options').querySelector('[data-action="add-provider-from-wizard"]')?.addEventListener('click', () => { closeWizard(); setView('providers'); openProviderForm(); });
}

function renderWizardStep() {
  $$('.wizard-step').forEach(step => step.classList.toggle('hidden', Number(step.dataset.step) !== state.cameraStep));
  $$('.step').forEach(step => step.classList.toggle('active', Number(step.dataset.step) <= state.cameraStep));
  $('#wizard-back').disabled = state.cameraStep === 1;
  $('#wizard-next').textContent = state.cameraStep === 3 ? t('finish') : t('next');
}

function nextWizardStep() {
  if (state.cameraStep === 1 && !$('input[name="camera-provider"]:checked')) return toast(t('requiredProvider'));
  if (state.cameraStep === 1 && !state.providerTests.has($('input[name="camera-provider"]:checked').value.toLowerCase())) return toast(t('providerTestRequired'));
  if (state.cameraStep === 2 && (!$('#camera-name').value.trim() || !$('#camera-serial').value.trim())) return toast(t('requiredCamera'));
  if (state.cameraStep < 3) { state.cameraStep += 1; renderWizardStep(); return; }
  saveCamera();
}

function closeWizard() { $('#camera-wizard').classList.add('hidden'); document.body.classList.remove('modal-open'); }

async function saveCamera() {
  const id = state.editingCameraId;
  const body = { name: $('#camera-name').value.trim(), brand: $('input[name="camera-provider"]:checked')?.value || '', serial: $('#camera-serial').value.trim(), username: $('#camera-username').value.trim(), password: $('#camera-password').value, port: Number($('#remote-port').value), local_port: Number($('#local-port').value), auto_start: $('#auto-start').checked };
  if (!body.name || !body.serial) return toast(t('requiredCamera'));
  const response = await api(id ? `/cameras/${id}` : '/cameras', { method: id ? 'PUT' : 'POST', body: JSON.stringify(body) });
  if (!response?.ok) return;
  const saved = await response.json();
  state.cameras = id ? state.cameras.map(camera => camera.id === id ? saved : camera) : [...state.cameras, saved];
  closeWizard(); setView('cameras'); toast(t('connectionSuccessful'));
}

function openProviderForm(id = null, suggestedName = '') {
  state.editingBrandId = id;
  document.body.classList.add('modal-open');
  $('#provider-form').classList.remove('hidden');
  const provider = state.brands.find(item => item.id === id);
  $('#provider-name').value = provider?.name || suggestedName || '';
  $('#provider-server').value = provider?.main_server || 'www.easy4ipcloud.com:8800';
  $('#provider-user').value = provider?.app_username || '';
  $('#provider-key').value = provider?.app_userkey || '';
  updateProviderTestButton();
  $('#provider-form').scrollIntoView({ behavior: 'smooth', block: 'start' });
}

function updateProviderTestButton() {
  const button = $('#provider-test');
  if (!button) return;
  const tested = state.providerTests.has($('#provider-name').value.trim().toLowerCase());
  button.textContent = t(tested ? 'verified' : 'testProvider');
  button.classList.toggle('button-success', tested);
}

function closeProviderForm() { $('#provider-form').classList.add('hidden'); document.body.classList.remove('modal-open'); }

async function saveProvider() {
  const id = state.editingBrandId;
  const body = { name: $('#provider-name').value.trim(), main_server: $('#provider-server').value.trim(), app_username: $('#provider-user').value.trim(), app_userkey: $('#provider-key').value.trim() };
  if (Object.values(body).some(value => !value)) return toast(t('requiredProvider'));
  const response = await api(id ? `/brands/${id}` : '/brands', { method: id ? 'PUT' : 'POST', body: JSON.stringify(body) });
  if (!response?.ok) return;
  const saved = await response.json();
  state.brands = id ? state.brands.map(item => item.id === id ? saved : item) : [...state.brands, saved];
  closeProviderForm(); renderProviders(); toast(t('connectionSuccessful'));
}

async function testProvider() {
  const body = { name: $('#provider-name').value.trim(), main_server: $('#provider-server').value.trim(), app_username: $('#provider-user').value.trim(), app_userkey: $('#provider-key').value.trim() };
  if (Object.values(body).some(value => !value)) return toast(t('requiredProvider'));
  const response = await api('/brands/test', { method: 'POST', body: JSON.stringify(body) });
  if (!response) return;
  const result = await response.json().catch(() => ({}));
  if (!response.ok) return toast(result.error || t('providerTestFailed'));
  state.providerTests.add(body.name.toLowerCase());
  updateProviderTestButton();
  renderProviders();
  toast(t('providerTestSuccess'));
}

async function deleteProvider(id) {
  const count = state.cameras.filter(camera => camera.brand === state.brands.find(brand => brand.id === id)?.name).length;
  if (!confirm(count ? t('brandInUse', { count }) : t('confirmDelete'))) return;
  const response = await api(`/brands/${id}`, { method: 'DELETE' });
  if (response?.ok) { state.brands = state.brands.filter(brand => brand.id !== id); renderProviders(); }
}

function openTokenForm(id = null) {
  state.editingTokenId = id;
  document.body.classList.add('modal-open');
  $('#token-form').classList.remove('hidden');
  const item = state.tokens.find(token => token.id === id);
  $('#token-name').value = item?.name || '';
  $('#token-expires').value = item?.expires_at?.slice(0, 16) || '';
  $('#token-enabled').checked = item ? item.enabled : true;
}

function closeTokenForm() { $('#token-form').classList.add('hidden'); document.body.classList.remove('modal-open'); }

async function saveToken() {
  const id = state.editingTokenId;
  const name = $('#token-name').value.trim();
  if (!name) return toast(t('requiredToken'));
  const body = { name, expires_at: $('#token-expires').value ? new Date($('#token-expires').value).toISOString() : null, enabled: $('#token-enabled').checked };
  const response = await api(id ? `/tokens/${id}` : '/tokens', { method: id ? 'PUT' : 'POST', body: JSON.stringify(body) });
  if (!response?.ok) return;
  const saved = await response.json();
  state.tokens = id ? state.tokens.map(item => item.id === id ? saved : item) : [...state.tokens, saved];
  closeTokenForm(); renderTokens(); if (saved.token) { await copyText(saved.token); toast(t('copied')); }
}

async function deleteToken(id) {
  if (!confirm(t('confirmDelete'))) return;
  const response = await api(`/tokens/${id}`, { method: 'DELETE' });
  if (response?.ok) { state.tokens = state.tokens.filter(item => item.id !== id); renderTokens(); }
}

async function toggleTunnel(id) {
  const status = getStatus(id);
  await api(`/cameras/${id}/${['running', 'starting'].includes(status) ? 'stop' : 'start'}`, { method: 'POST' });
  setTimeout(loadTunnels, 600);
}

async function deleteCamera(id) {
  if (!confirm(t('confirmDelete'))) return;
  const response = await api(`/cameras/${id}`, { method: 'DELETE' });
  if (response?.ok) { state.cameras = state.cameras.filter(camera => camera.id !== id); renderAll(); }
}

async function copyText(value) {
  if (!value || value === t('notAvailable')) return;
  try { await navigator.clipboard.writeText(value); toast(t('copied')); } catch { toast(value); }
}

$('#language').addEventListener('change', async event => { state.lang = event.target.value; localStorage.setItem('camrelay_language', state.lang); await loadLocale(); renderAll(); });
$('#theme-toggle').addEventListener('click', () => applyTheme(state.theme === 'dark' ? 'light' : 'dark'));
$$('.nav-button').forEach(button => button.addEventListener('click', () => setView(button.dataset.view)));
$('[data-view-link="cameras"]').addEventListener('click', () => setView('cameras'));
$('[data-view-link="about"]').addEventListener('click', () => setView('about'));
$('[data-view-link="recordings"]')?.addEventListener('click', () => setView('recordings'));
$('#logout').addEventListener('click', logout);
$('#add-camera').addEventListener('click', () => openCameraWizard());
$('#refresh-recordings').addEventListener('click', loadRecordings);
$('#recording-camera-filter').addEventListener('change', renderRecordings);
$('#recording-status-filter').addEventListener('change', renderRecordings);
$('#close-player').addEventListener('click', closeRecordingPlayer);
$('#wizard-next').addEventListener('click', nextWizardStep);
$('#wizard-back').addEventListener('click', () => { if (state.cameraStep > 1) { state.cameraStep -= 1; renderWizardStep(); } });
$('#wizard-cancel').addEventListener('click', closeWizard);
$('#add-provider').addEventListener('click', () => openProviderForm());
$('#provider-cancel').addEventListener('click', closeProviderForm);
$('#provider-test').addEventListener('click', testProvider);
['#provider-name', '#provider-server', '#provider-user', '#provider-key'].forEach(selector => $(selector).addEventListener('input', () => {
  state.providerTests.delete($('#provider-name').value.trim().toLowerCase());
  updateProviderTestButton();
}));
$('#provider-save').addEventListener('click', saveProvider);
$('#add-token').addEventListener('click', () => openTokenForm());
$('#token-cancel').addEventListener('click', closeTokenForm);
$('#token-save').addEventListener('click', saveToken);
const scrollTopButton = $('#scroll-top');
window.addEventListener('scroll', () => scrollTopButton.classList.toggle('visible', window.scrollY > 260));
scrollTopButton.addEventListener('click', () => window.scrollTo({ top: 0, behavior: 'smooth' }));
window.addEventListener('popstate', () => {
  const view = viewFromLocation();
  if (state.token && view) setView(view, false);
  else if (!state.token) showLogin();
});
$('#login-form').addEventListener('submit', async event => {
  event.preventDefault();
  try {
    const response = await fetch('/api/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ username: $('#login-username').value, password: $('#login-password').value }) });
    if (!response.ok) { $('#login-error').textContent = t('invalidCredentials'); return; }
    state.token = (await response.json()).token; localStorage.setItem('camrelay_token', state.token); $('#login-error').textContent = ''; showApp();
  } catch { $('#login-error').textContent = t('networkError'); }
});

(async function init() { state.activeView = viewFromLocation() || 'dashboard'; await loadLocale(); if (state.token && viewFromLocation()) showApp(); else showLogin(); })();
