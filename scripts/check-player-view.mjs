import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import ts from 'typescript';
const source = await readFile(new URL('../src/player-view.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { PlayerView, PlayerFrameLoop } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);

// Small DOM/window doubles preserve identity, actual parentage and ownerDocument.
// Moving a node adopts all descendants; replacing the opener workspace removes
// the return marker just as real application navigation does.
class Node extends EventTarget {
  constructor(document, tag = '') { super(); this.ownerDocument = document; this.tagName = tag; this.children = []; this.parentNode = null; this.dataset = {}; this.attributes = {}; this.textContent = ''; }
  adopt(document) { this.ownerDocument = document; for (const child of this.children) child.adopt(document); }
  append(...nodes) { for (const node of nodes) { node.remove(); node.parentNode = this; node.adopt(this.ownerDocument); this.children.push(node); } }
  replaceWith(node) {
    if (!this.parentNode) return;
    const parent = this.parentNode, index = parent.children.indexOf(this); node.remove();
    parent.children[index] = node; node.parentNode = parent; node.adopt(parent.ownerDocument); this.parentNode = null;
  }
  replaceChildren(...nodes) { for (const child of [...this.children]) child.remove(); this.append(...nodes); }
  remove() { if (this.parentNode) this.parentNode.children.splice(this.parentNode.children.indexOf(this), 1); this.parentNode = null; }
  get isConnected() { return this === this.ownerDocument.body || this === this.ownerDocument.head || Boolean(this.parentNode?.isConnected); }
  setAttribute(key, value) { this.attributes[key] = value; }
  getBoundingClientRect() { return { width: 1100, height: 700 }; }
  querySelector(selector) { for (const child of this.children) { if (selector === '[data-player-window-name]' && 'playerWindowName' in child.dataset) return child; const found = child.querySelector(selector); if (found) return found; } return null; }
  text() { return this.textContent + this.children.map(child => child.text()).join(' '); }
}
class Document extends EventTarget {
  constructor(window) { super(); this.defaultView = window; this.hidden = false; this.baseURI = 'https://example.test/agar/'; this.head = new Node(this, 'head'); this.body = new Node(this, 'body'); this.styleSheets = [{ cssRules: [{ cssText: '.player-stage { color: white; }' }] }]; }
  createElement(tag) { return new Node(this, tag); }
  createComment() { return new Node(this, '#comment'); }
  createDocumentFragment() { return new Node(this, '#fragment'); }
  querySelector(selector) { return this.body.querySelector(selector); }
}
class BrowserWindow extends EventTarget {
  constructor(name) { super(); this.name = name; this.document = new Document(this); this.closed = false; this.frames = new Map(); this.nextFrame = 0; this.timers = new Map(); this.nextTimer = 0; this.focused = false; this.open = () => null; this.Event = class extends Event {}; this.CustomEvent = class extends Event { constructor(name, data) { super(name); this.detail = data?.detail; } }; }
  focus() { this.focused = true; }
  close() { if (this.closed) return; this.closed = true; this.dispatchEvent(new this.Event('pagehide')); }
  requestAnimationFrame(callback) { const id = ++this.nextFrame; this.frames.set(id, callback); return id; }
  cancelAnimationFrame(id) { this.frames.delete(id); }
  fireFrame(timestamp = -10000) { const entry = this.frames.entries().next().value; if (!entry) return false; this.frames.delete(entry[0]); entry[1](timestamp); return true; }
  setTimeout(callback, delay) { const id = ++this.nextTimer; this.timers.set(id, { callback, delay }); return id; }
  clearTimeout(id) { this.timers.delete(id); }
  fireTimer() { const entry = this.timers.entries().next().value; if (!entry) return false; this.timers.delete(entry[0]); entry[1].callback(); return true; }
}
function sendInput(target, type, fields = {}) {
  const event = new Event(type, { cancelable: true });
  for (const [name, value] of Object.entries(fields)) Object.defineProperty(event, name, { value });
  target.dispatchEvent(event);
  return event;
}
const createPlayer = opener => { const host = opener.document.createElement('div'), player = opener.document.createElement('section'), canvas = opener.document.createElement('canvas'); player.append(canvas); host.append(player); opener.document.body.append(host); return { host, player, canvas }; };

// Document PiP moves the same player/canvas, copies styles, and keeps one state owner.
{
  const opener = new BrowserWindow('opener'), floating = new BrowserWindow('pip'), { host, player, canvas } = createPlayer(opener);
  const engine = { ticks: 123 }; player.engine = engine;
  let requests = 0, binds = 0, unbinds = 0; const changes = [];
  opener.documentPictureInPicture = { requestWindow: async options => {
    requests++;
    assert.ok(options.width <= 1000);
    assert.equal(options.disallowReturnToOpener, true, 'Request one in-player return control');
    return floating;
  } };
  const view = new PlayerView(opener, { bindDocument: doc => { assert.equal(doc, floating.document); binds++; return () => unbinds++; }, changed: reason => changes.push(reason), visibilityChanged: () => {} });
  assert.equal(view.supportsAlwaysOnTop, true);
  assert.equal(await view.open(player, 'Ecosystem'), 'picture-in-picture');
  assert.equal(requests, 1); assert.equal(binds, 1); assert.equal(player.ownerDocument, floating.document); assert.equal(canvas.ownerDocument, floating.document);
  assert.equal(player.engine, engine); assert.equal(view.visible, true);
  assert.equal(floating.document.title, 'Ecosystem · agar player');
  assert.equal(floating.document.body.children[0].children.some(node => node.tagName === 'header'), false, 'The canvas fills the shell without a permanent child-window header');
  const state = () => floating.document.body.dataset.playerControls;
  assert.equal(state(), 'hidden', 'A newly opened player starts with the simulation visible');
  floating.innerWidth = 800; floating.innerHeight = 600;
  const pointer = { pointerType: 'mouse', clientX: 30, clientY: 40, target: canvas };
  sendInput(floating.document, 'pointerover', pointer);
  assert.equal(state(), 'pointer');
  sendInput(floating.document, 'pointerout', { ...pointer, relatedTarget: canvas });
  assert.equal(state(), 'pointer', 'Moving between elements inside the window keeps controls available');
  sendInput(floating.document, 'pointermove', { ...pointer, clientX: -2 });
  assert.equal(state(), 'hidden', 'An object drag can deliver captured moves outside the viewport');
  sendInput(floating.document, 'keydown', { key: 'Tab', target: canvas });
  assert.equal(state(), 'keyboard', 'Keyboard users can reveal the controls without pointer input');
  sendInput(floating, 'blur');
  assert.equal(state(), 'hidden');
  const touch = { ...pointer, pointerType: 'touch', pointerId: 7 };
  assert.ok(sendInput(floating.document, 'pointerdown', touch).defaultPrevented);
  assert.equal(state(), 'touch');
  assert.ok(sendInput(floating.document, 'pointerup', touch).defaultPrevented);
  assert.ok(sendInput(floating.document, 'click', { target: canvas, detail: 1 }).defaultPrevented, 'A wake tap cannot also activate the canvas');
  const outsideMouse = { ...pointer, clientX: -2, relatedTarget: null };
  for (const type of ['pointerover', 'pointermove', 'pointerout']) {
    sendInput(floating.document, type, outsideMouse);
    assert.equal(state(), 'touch', `A stationary mouse outside the window cannot dismiss touch controls on ${type}`);
  }
  assert.equal(sendInput(floating.document, 'pointerdown', touch).defaultPrevented, false);
  sendInput(floating.document, 'pointerup', touch);
  assert.equal(sendInput(floating.document, 'click', { target: canvas, detail: 1 }).defaultPrevented, false, 'The next tap is available for normal play');
  sendInput(floating.document, 'pointerover', { ...pointer, relatedTarget: null });
  assert.equal(state(), 'pointer', 'A mouse entering the viewport takes over from touch');
  sendInput(floating.document, 'pointerout', outsideMouse);
  assert.equal(state(), 'hidden', 'A subsequent real mouse exit hides pointer controls');
  sendInput(floating.document, 'keydown', { key: 'Tab', target: canvas });

  const hide = floating.document.createElement('button');
  hide.closest = selector => selector === '[data-player-hide]' ? hide : null;
  floating.document.activeElement = hide;
  floating.document.getElementById = id => id === 'world' ? canvas : null;
  canvas.focus = () => { floating.document.activeElement = canvas; };
  sendInput(floating.document, 'click', { target: hide, detail: 0 });
  assert.equal(state(), 'hidden');
  assert.equal(floating.document.activeElement, canvas, 'Hiding controls from the keyboard returns focus to the world');
  assert.ok(floating.document.head.children.some(node => node.tagName === 'style'));
  await view.open(player, 'Ecosystem'); assert.equal(requests, 1); assert.equal(floating.focused, true);
  opener.document.hidden = true; assert.equal(view.visible, true, 'Hidden opener must not hide PiP');
  // Main app navigates to a different section; the same popped-out player survives.
  host.remove(); opener.document.body.append(opener.document.createElement('main'));
  assert.equal(player.isConnected, true); assert.equal(player.ownerDocument, floating.document); assert.equal(engine.ticks, 123);
  view.setTitle('Renamed run'); assert.equal(floating.document.title, 'Renamed run · agar player');
  floating.close();
  assert.equal(view.isOpen, false); assert.equal(unbinds, 1); assert.deepEqual(changes, [undefined, 'closed']);
  assert.equal(player.ownerDocument, opener.document, 'Closed window adopts node back even after main navigation');
  assert.equal(canvas.ownerDocument, opener.document); assert.equal(player.engine, engine);
  const closedState = state();
  sendInput(floating.document, 'pointermove', pointer);
  assert.equal(state(), closedState, 'An old window cannot show chrome after its listeners are detached');
  opener.document.body.append(player); assert.equal(player.isConnected, true);
  view.close(); assert.equal(unbinds, 1, 'Closing twice is harmless');
}

// The ordinary-window fallback uses the same shell and restores the exact slot.
{
  const opener = new BrowserWindow('fallback-opener'), floating = new BrowserWindow('popup'), { host, player } = createPlayer(opener);
  opener.open = (url, name, features) => { assert.equal(name, '_blank'); assert.match(features, /popup/); return floating; };
  const reasons = []; const view = new PlayerView(opener, { bindDocument: () => () => {}, changed: reason => reasons.push(reason), visibilityChanged: () => {} });
  assert.equal(view.supportsAlwaysOnTop, false); assert.equal(await view.open(player, 'Run'), 'window');
  assert.equal(floating.document.body.dataset.playerWindowMode, 'window');
  assert.equal(floating.document.title, 'Run · agar player');
  view.close(); assert.equal(host.children[0], player); assert.equal(floating.closed, true); assert.deepEqual(reasons, [undefined, 'return']);
  const reopened = new BrowserWindow('second-popup'); opener.open = () => reopened;
  await view.open(player, 'Run'); assert.equal(player.ownerDocument, reopened.document);
  view.close('session'); assert.equal(host.children[0], player); assert.equal(reopened.closed, true);
  assert.deepEqual(reasons, [undefined, 'return', undefined, 'session']);
}

// Failed requests and stale asynchronous opens never steal the current player.
{
  const opener = new BrowserWindow('failed-opener'), { host, player } = createPlayer(opener);
  const view = new PlayerView(opener, { bindDocument: () => () => {}, changed: () => {}, visibilityChanged: () => {} });
  await assert.rejects(view.open(player, 'Run'), /blocked/); assert.equal(host.children[0], player); assert.equal(view.isOpen, false);
  let resolve; const floating = new BrowserWindow('stale');
  opener.documentPictureInPicture = { requestWindow: () => new Promise(done => { resolve = done; }) };
  const pending = view.open(player, 'Run', () => false); resolve(floating);
  await assert.rejects(pending, /player changed/); assert.equal(floating.closed, true); assert.equal(host.children[0], player);
  const rejected = new BrowserWindow('rejected');
  opener.documentPictureInPicture.requestWindow = async () => { throw new Error('Permission denied'); };
  await assert.rejects(view.open(player, 'Run'), /Permission denied/); assert.equal(view.isOpen, false); assert.equal(rejected.closed, false);
}

// Roll back ownership and bindings if setup fails after the node has moved.
{
  const opener = new BrowserWindow('rollback-opener'), floating = new BrowserWindow('rollback-pip'), { host, player, canvas } = createPlayer(opener);
  opener.documentPictureInPicture = { requestWindow: async () => floating };
  const view = new PlayerView(opener, { bindDocument: () => { throw new Error('Binding failed'); }, changed: () => {}, visibilityChanged: () => {} });
  await assert.rejects(view.open(player, 'Run'), /Binding failed/);
  assert.equal(host.children[0], player); assert.equal(canvas.ownerDocument, opener.document); assert.equal(floating.closed, true); assert.equal(view.isOpen, false);
}

// Visible PiP frames keep executing when the opener has no animation callbacks.
// Different window timestamps are deliberately ignored in favor of one clock.
{
  const opener = new BrowserWindow('clock-opener'), floating = new BrowserWindow('clock-pip');
  let activeWindow = opener, now = 1000, ticks = 0, paused = false, section = 'simulation';
  const sampled = [];
  const loop = new PlayerFrameLoop(() => activeWindow, time => { sampled.push(time); if (!paused) ticks++; }, () => now);
  loop.start(); loop.start(); assert.equal(opener.frames.size, 1);
  const stale = [...opener.frames.values()][0];
  opener.fireFrame(10); assert.equal(ticks, 1); assert.deepEqual(sampled, [1000]);
  activeWindow = floating; opener.document.hidden = true; section = 'genomes'; loop.refresh();
  assert.equal(opener.frames.size, 0); assert.equal(floating.frames.size, 1);
  stale(20); assert.equal(ticks, 1, 'Old window callbacks cannot advance after host switches');
  for (let index = 0; index < 5; index++) { now += 16; floating.fireFrame(1e9 + index); }
  assert.equal(section, 'genomes'); assert.equal(ticks, 6, 'Main app navigation does not replace or halt the shared loop');
  paused = true; floating.fireFrame(); assert.equal(ticks, 6, 'Explicit pause still works');
  paused = false; now += 16; floating.fireFrame(); assert.equal(ticks, 7);
  activeWindow = opener; opener.document.hidden = false; loop.refresh(); assert.equal(floating.frames.size, 0); assert.equal(opener.frames.size, 1);
  loop.stop(); assert.equal(opener.frames.size, 0); loop.stop();
  loop.start(); assert.equal(opener.frames.size, 1); loop.stop();
}

// Hidden players still advance through exactly one timer. Visibility changes,
// docking and shutdown cancel the old callback even if it was already queued.
{
  const opener = new BrowserWindow('hidden-opener'), floating = new BrowserWindow('hidden-pip'), { player } = createPlayer(opener);
  opener.document.hidden = true; floating.document.hidden = true;
  opener.documentPictureInPicture = { requestWindow: async () => floating };
  let loop, now = 1000, ticks = 0;
  const view = new PlayerView(opener, { bindDocument: () => () => {}, changed: () => loop?.refresh(), visibilityChanged: () => loop?.refresh() });
  await view.open(player, 'Background run');
  loop = new PlayerFrameLoop(() => view.window ?? opener, () => ticks++, () => now, () => view.isOpen);
  loop.start(); loop.refresh();
  assert.equal(floating.frames.size, 0); assert.equal(floating.timers.size, 1);
  assert.equal([...floating.timers.values()][0].delay, 100, 'Fallback remains bounded rather than spinning');
  for (let index = 0; index < 4; index++) { now += 100; floating.fireTimer(); assert.equal(floating.timers.size, 1); }
  assert.equal(ticks, 4, 'Both hidden windows still advance an open player');
  const hiddenCallback = [...floating.timers.values()][0].callback;
  floating.document.hidden = false; floating.document.dispatchEvent(new floating.Event('visibilitychange'));
  assert.equal(floating.timers.size, 0); assert.equal(floating.frames.size, 1);
  hiddenCallback(); assert.equal(ticks, 4, 'A stale timer cannot advance after visibility returns');
  now += 16; floating.fireFrame(); assert.equal(ticks, 5);
  const visibleCallback = [...floating.frames.values()][0];
  floating.document.hidden = true; floating.document.dispatchEvent(new floating.Event('visibilitychange'));
  assert.equal(floating.frames.size, 0); assert.equal(floating.timers.size, 1);
  visibleCallback(); assert.equal(ticks, 5, 'A stale visible frame cannot duplicate a hidden tick');
  const closingCallback = [...floating.timers.values()][0].callback;
  view.close();
  assert.equal(floating.timers.size, 0); assert.equal(floating.frames.size, 0);
  assert.equal(opener.timers.size, 0); assert.equal(opener.frames.size, 1, 'A docked hidden tab keeps its normal paused-rAF behavior');
  closingCallback(); assert.equal(ticks, 5);
  const reopened = new BrowserWindow('hidden-reopened'); reopened.document.hidden = true;
  opener.documentPictureInPicture.requestWindow = async () => reopened;
  await view.open(player, 'Background run');
  assert.equal(opener.frames.size, 0); assert.equal(reopened.timers.size, 1);
  const stoppedCallback = [...reopened.timers.values()][0].callback;
  loop.stop(); assert.equal(reopened.timers.size, 0); stoppedCallback(); assert.equal(ticks, 5);
  view.close(); assert.equal(opener.frames.size, 0, 'Closing after stop cannot restart the loop');
}

// Exercise the real application gates, rather than a second implementation of
// them: catalog navigation/edits retain the renderer and the independent work
// loop can advance the same session until the user explicitly pauses it.
{
  const appSource = await readFile(new URL('../src/app.ts', import.meta.url), 'utf8');
  const parsed = ts.createSourceFile('app.ts', appSource, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TS);
  const pauseSaveStates = [];
  const names = ['renderView', 'playerPlaceholderMarkup', 'navigate', 'frame', 'playerIsVisible', 'viewPresentationOptions', 'simulationWork', 'advanceSimulation', 'togglePlayback', 'playerVisibilityChanged', 'handleUiClick', 'handleUiInput', 'handleUiChange', 'handleUiKeydown', 'handleUiDismissMenus', 'bindUiDocument'];
  const functions = names.map(name => {
    const declaration = parsed.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
    assert.ok(declaration, `Application function ${name} is present`); return declaration.getText(parsed);
  }).join('\n');
  const appCode = ts.transpileModule(functions, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  const opener = new BrowserWindow('application'), floating = new BrowserWindow('application-pip');
  opener.document.querySelectorAll = () => [];
  const workspace = opener.document.createElement('main'), player = floating.document.createElement('section');
  const calls = { advances: [], renders: 0, ui: 0, refreshes: 0, flushes: 0, checkpoints: 0, actions: [] };
  const session = { metadata: { name: 'Run', paused: false, speed: 1 }, world: { tick: 123 }, busy: false, pending: null, revision: 0, savedRevision: 0,
    touch() { this.revision++; },
    advance(steps, options) { calls.advances.push({ steps, options }); this.world.tick += steps; return Promise.resolve(); },
    refresh() { calls.refreshes++; return Promise.resolve(); }, flush() { calls.flushes++; return Promise.resolve(); },
    checkpoint() { calls.checkpoints++; this.savedRevision = this.revision; return Promise.resolve(); },
  };
  const renderer = { dispose() { assert.fail('Main navigation must not dispose a popped-out renderer'); }, render() { calls.renders++; } };
  const editor = { destroy() { assert.fail('Main navigation must not destroy the live player inspector'); } };
  const fail = error => { throw error instanceof Error ? error : new Error(String(error)); };
  const context = vm.createContext({
    document: opener.document, window: opener, performance: { now: () => 0 },
    ready: true, transitioning: false, unloading: false, prefs: { tab: 'simulation', simulationView: 'player', overlay: 'none' },
    active: session, renderer, inspectorProgram: editor, playerElement: player, restoringPlayer: false,
    inspectorEpoch: 0, selectedId: null, cellInspection: null, catalogQuery: '', environmentOpen: false, liveGasesOpen: false,
    lastPresentation: 0, lastMetrics: 0, lastEnvironment: 0, lastCheckpoint: 0, lastUi: 0, lifecycleFlushes: 0, saveFailed: false,
    playerView: { isOpen: true, visible: true, setTitle() {} },
    frameLoop: { refresh() {} }, simulationLoop: { refresh() {}, reset() {} }, flushLifecycle: async () => {},
    $: id => id === 'workspace' ? workspace : null, queryUi: () => null, esc: value => value, icon: () => '',
    renderCatalog: () => '<section>Catalog</section>', renderCatalogCards() {}, refreshCatalogs: async () => {}, persistPreferences() {},
    metricsMarkup: () => '<section>Metrics</section>', settingsMarkup: () => '<section>Settings</section>',
    renderLiveEnvironment() {}, updateSimulationUi() { calls.ui++; }, updatePlayback() {}, saveStatus(state, _error, scope) { if (scope === 'pause-checkpoint') pauseSaveStates.push(state); }, runError: (_session, error) => fail(error), toast: fail, readableError: String,
  });
  vm.runInContext(appCode, context);
  context.playerView.mode = 'window';
  assert.match(context.playerPlaceholderMarkup(), /cannot keep it always on top/i, 'The main tab explains the ordinary-window limitation');
  context.playerView.mode = 'picture-in-picture';
  assert.match(context.playerPlaceholderMarkup(), /data-action="return-player"/, 'The opener retains an accessible way to dock the player');
  for (const tab of ['genomes', 'environments']) {
    await context.navigate(tab);
    // Saving a catalog edit renders its main-app view again.
    context.renderView();
    assert.equal(context.playerElement, player); assert.equal(context.renderer, renderer); assert.equal(context.inspectorProgram, editor);
    assert.equal(context.active, session); assert.equal(context.transitioning, false);
  }
  await context.navigate('simulation');
  for (const view of ['metrics', 'settings', 'genomes', 'player']) { context.prefs.simulationView = view; context.renderView(); }
  assert.equal(context.playerElement, player); assert.equal(context.renderer, renderer);
  let now = 1000;
  const frame = () => {
    context.frame(now += 100);
    const work = context.simulationWork();
    if (work && !work.busy) void context.advanceSimulation(work.target, 3, now);
  };
  opener.document.hidden = true;
  for (const tab of ['genomes', 'environments', 'simulation']) {
    context.prefs.tab = tab; const before = calls.advances.length; frame();
    assert.equal(calls.advances.length, before + 1, `${tab}: visible PiP advances with a hidden opener`);
    assert.equal(calls.advances.at(-1).options.frame, true);
    assert.equal(calls.advances.at(-1).options.environment, 'metadata', 'No overlay exports only object/config metadata');
  }
  session.metadata.paused = true; let before = calls.advances.length; frame(); assert.equal(calls.advances.length, before);
  session.metadata.paused = false; frame(); assert.equal(calls.advances.length, before + 1);
  context.playerView.visible = false; before = calls.advances.length;
  const drawn = calls.renders, painted = calls.ui;
  frame(); assert.equal(calls.advances.length, before + 1, 'Both hidden surfaces still advance an open player');
  assert.equal(calls.renders, drawn); assert.equal(calls.ui, painted, 'Hidden frames skip rendering and inspector work');
  assert.equal(calls.advances.at(-1).options.environment, false); assert.equal(calls.advances.at(-1).options.metrics, false);
  assert.equal(calls.advances.at(-1).options.frame, false, 'Hidden stepping does not export entities');
  session.busy = true; before = calls.advances.length; frame(); assert.equal(calls.advances.length, before, 'Only one advancing batch may be in flight'); session.busy = false;
  const saved = calls.checkpoints; session.touch(); now += 3600000; frame();
  assert.ok(calls.advances.at(-1).steps <= 8, 'A delayed callback never triggers wall-clock catch-up');
  assert.equal(calls.checkpoints, saved + 1, 'Hidden playback retains periodic checkpoints');
  context.playerView.isOpen = false; before = calls.advances.length; frame(); assert.equal(calls.advances.length, before, 'A hidden tab without a pop-out stays paused');
  context.playerView.isOpen = true; opener.document.hidden = false; frame(); assert.equal(calls.advances.length, before + 1, 'Visible main app can drive an open hidden player');
  const refreshed = calls.refreshes; session.metadata.paused = true; context.playerView.visible = true;
  context.playerVisibilityChanged(); assert.equal(calls.refreshes, refreshed + 1, 'Returning refreshes hidden projections even when paused');
  context.prefs.overlay = 'oxygen';
  assert.equal(context.viewPresentationOptions().environment, true, 'Visible overlays request their field samples');
  context.playerView.visible = false; context.playerView.isOpen = false; context.prefs.tab = 'simulation'; context.prefs.simulationView = 'metrics';
  assert.deepEqual(JSON.parse(JSON.stringify(context.viewPresentationOptions())), { frame: false, metrics: true, environment: 'metadata' }, 'Metrics needs no cell arrays or full field grid');
  session.metadata.paused = false;
  context.playerView.isOpen = true; context.playerView.visible = true; context.prefs.tab = 'environments';

  // Delegation accepts elements/events from another window without relying on
  // the opener's Element constructor, and removing it prevents duplicate work.
  const actions = [];
  context.action = name => { calls.actions.push(name); const operation = name === 'play' ? context.togglePlayback() : Promise.resolve(); actions.push(operation); return operation; };
  const button = floating.document.createElement('button'); button.dataset.action = 'play';
  button.closest = selector => selector === 'button' ? button : null;
  const click = () => { const event = new floating.Event('click'); Object.defineProperty(event, 'target', { value: button }); floating.document.dispatchEvent(event); };
  const unbind = context.bindUiDocument(floating.document);
  click(); assert.deepEqual(pauseSaveStates, ['saving']);
  await Promise.all(actions);
  assert.deepEqual(pauseSaveStates, ['saving','saved']);
  assert.equal(session.metadata.paused, true); assert.deepEqual(calls.actions, ['play']);
  assert.equal(context.transitioning, false); before = calls.advances.length; frame(); assert.equal(calls.advances.length, before);
  click(); await Promise.all(actions); assert.equal(session.metadata.paused, false); frame(); assert.equal(calls.advances.length, before + 1);
  unbind(); click(); assert.deepEqual(calls.actions, ['play', 'play'], 'Closed-window delegation is detached');
  assert.ok(calls.flushes >= 4, 'Navigation and explicit pause finish accepted work');
}
console.log('Player view: shared DOM/state, app navigation, cross-window controls, explicit pause, hidden-player timer stepping, bounded batches/checkpoints, visibility cancellation and close/stop cleanup passed.');
