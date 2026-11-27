import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { JSDOM } from 'jsdom';
import { createServer } from 'vite';
import { mockIPC } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import { catalogFixture } from './fixtures/catalog.mjs';

let server, dom, deferred, failNextPage = false;
const calls = [];
const queries = () => calls.filter(([cmd]) => cmd === 'search_catalog').map(([, args]) => args.query);
const $ = (selector, parent = document) => parent.querySelector(selector);
const wait = (ms) => new Promise(resolve => setTimeout(resolve, ms));
async function until(fn, message) {
  for (let i = 0; i < 100; i++) { const found = fn(); if (found) return found; await wait(12); }
  assert.fail(message);
}
function button(parent, text) {
  const result = [...parent.querySelectorAll('button')].find(el => el.textContent.includes(text));
  assert.ok(result, `Missing button: ${text}`); return result;
}
function type(input, value) {
  Object.getOwnPropertyDescriptor(dom.window.HTMLInputElement.prototype, 'value').set.call(input, value);
  input.dispatchEvent(new dom.window.Event('input', { bubbles: true }));
}
before(async () => {
  dom = new JSDOM('<div id="root"></div>', { pretendToBeVisual: true, url: 'http://localhost' });
  globalThis.window = dom.window; globalThis.document = window.document;
  globalThis.ResizeObserver = window.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} };
  for (const [prop, value] of Object.entries({ clientWidth: 680, clientHeight: 620 })) {
    Object.defineProperty(window.HTMLElement.prototype, prop, { get: () => value });
  }
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'search_catalog' && args.query.text === 'lento') return new Promise(resolve => { deferred = () => resolve({ ...catalogFixture(cmd, args), totalModels: 0, items: [] }); });
    if (cmd === 'search_catalog' && args.query.offset > 0 && failNextPage) {
      failNextPage = false; throw { code: 'DATABASE_ERROR', message: 'Pagina temporaneamente non disponibile' };
    }
    return catalogFixture(cmd, args);
  }, { shouldMockEvents: true });
  server = await createServer({ server: { middlewareMode: true, ws: false }, appType: 'custom' });
  await server.ssrLoadModule('/src/main.tsx');
});
after(async () => { await server?.close(); dom?.window.close(); });

test('browse, search, inspect and switch views during a scan, without opening user files', async () => {
  const input = await until(() => $('input[type="search"]'), 'The app must mount the catalog search');
  await until(() => $('main')?.textContent.includes('Quadrante'), 'Root folders show representative titles');
  assert.equal(queries()[0].viewMode, 'models');
  assert.equal(queries()[0].folderId, null);
  assert.ok(!document.querySelector('canvas,img'));
  button($('main'), 'Atelier').click();
  await until(() => $('main').textContent.includes('Strumenti') && $('main').textContent.includes('Astrolabio 0'), 'Mixed folders and direct models');
  assert.ok(queries().some(q => q.folderId === 10 && q.sourceId === 1));
  button($('main'), 'Strumenti').click();
  await until(() => queries().at(-1)?.folderId === 12 && $('main').textContent.includes('Astrolabio 0'), 'Scope nested folder');
  button($('main'), 'Astrolabio 0').click();
  await until(() => document.body.textContent.includes('interno/disco.stl'), 'Inspector shows archive contents');
  assert.ok(document.body.textContent.includes('local://D:/Atelier/Strumenti/Astro_0.zip'));
  assert.ok(calls.some(([cmd, args]) => cmd === 'get_model_detail' && args.modelId === 20));
  const detailsBefore = calls.filter(([cmd]) => cmd === 'get_model_detail').length;
  const pagesBefore = queries().length;
  await emit('indexer://batch_updated', { sourceId: 1, updatedFolderIds: [12], updatedModelIds: [999] });
  await until(() => queries().length > pagesBefore, 'Catalog refresh follows a committed batch');
  assert.equal(calls.filter(([cmd]) => cmd === 'get_model_detail').length, detailsBefore, 'Unrelated batches must not reload the complete selected archive');
  assert.equal(document.activeElement, $('#inspector h2'));
  document.activeElement.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  await until(() => !$('#inspector').textContent.includes('interno/disco.stl'), 'Escape dismisses Inspector');
  await emit('indexer://progress', { sourceId: 1, pass: 2, phaseLabel: 'Analisi ZIP', processed: 4, total: 30, currentItem: 'parte.zip' });
  await emit('indexer://error', { sourceId: 1, recoverable: true, message: 'Archivio incompleto', path: 'parte.zip' });
  await until(() => document.body.textContent.includes('Archivio incompleto') && $('[aria-label="Annulla scansione"]'), 'Recoverable warnings and progress');
  $('[aria-label="Annulla scansione"]').click();
  await until(() => calls.some(([cmd]) => cmd === 'cancel_index_scan'), 'Cancel without blocking navigation');
  type(input, 'lento'); await until(() => deferred, 'Start slow query');
  type(input, 'Astrolabio'); await until(() => queries().at(-1)?.text === 'Astrolabio', 'Search scoped subtree');
  deferred(); await wait(20);
  assert.ok($('main').textContent.includes('Astrolabio 0'), 'Ignore out-of-order search response');
  button(document, 'Singoli File').click();
  await until(() => $('main').textContent.includes('Strumenti/Astrolabio.png'), 'Standalone image visible with contextual path');
  assert.ok(queries().some(q => q.text === 'Astrolabio' && q.folderId === 12 && q.viewMode === 'files'));
  assert.equal(input.value, 'Astrolabio');
  assert.ok($('main').textContent.includes('Cloud / Online-Only'));
  assert.ok(![...$('main').querySelectorAll('button:not(:disabled)')].some(el => el.textContent.includes('Astrolabio.png')));
  const detailCalls = calls.filter(([cmd]) => cmd === 'get_model_detail').length;
  $('main article').click(); await wait(20);
  assert.equal(calls.filter(([cmd]) => cmd === 'get_model_detail').length, detailCalls, 'Standalone image never requests model details');
  assert.ok(!calls.some(([cmd, args]) => cmd === 'get_model_detail' && args.modelId == null));
  const count = queries().length;
  await emit('indexer://batch_updated', { sourceId: 1, updatedFolderIds: [12], updatedModelIds: [20] });
  await until(() => queries().length > count, 'Refresh visible SQLite query after committed batch');
  await emit('indexer://completed', { sourceId: 1, errorsCount: 1, cancelled: false, durationMs: 100, modelsIndexed: 5, filesIndexed: 8 });
  button(document, 'Modelli / Pack').click();
  await until(() => $('main').textContent.includes('Astrolabio 0'), 'Restore model view');
  const scroll = $('[aria-label="Risultati del catalogo"]');
  assert.ok(scroll); failNextPage = true; scroll.scrollTop = 27000; scroll.dispatchEvent(new window.Event('scroll'));
  await until(() => queries().some(q => q.offset > 0), 'Scroll fetches later SQLite pages');
  await until(() => $('main').textContent.includes('Pagina temporaneamente non disponibile'), 'Page failure can be retried');
  button($('main'), 'Riprova').click();
  await until(() => /Astrolabio [2-9]\d\d/.test($('main').textContent), 'Deep window renders real later records');
  const deep = queries().at(-1);
  const deepQueries = () => queries().filter(q => q.offset === deep.offset && q.text === deep.text).length;
  const beforeBoundary = deepQueries();
  // Approach the next page with the previous page still visible in the overscan window.
  const renderedRows = [...scroll.querySelectorAll('.virtual-row')];
  const rowHeight = renderedRows[0].getBoundingClientRect().height || Number.parseFloat(renderedRows[0].style.height);
  const columns = renderedRows.at(-1).children.length;
  scroll.scrollTop = (deep.offset + deep.limit) / columns * rowHeight - 200;
  scroll.dispatchEvent(new window.Event('scroll'));
  await until(() => queries().some(q => q.offset === deep.offset + deep.limit), 'Prefetch next visible page');
  assert.equal(deepQueries(), beforeBoundary, 'Overlapping windows reuse the already visible page');
  assert.ok(queries().every(q => q.limit > 0 && q.limit < 500));
  assert.ok($('main').querySelectorAll('button').length < 100, 'DOM bounded independently of 50,010 hits');
  button(document, 'Scansiona tutto').click();
  await until(() => calls.some(([cmd, args]) => cmd === 'start_index_scan' && !args?.sourceId), 'Scan all without source argument');
  button($('#sources'), 'Locali').click();
  await until(() => queries().at(-1)?.sourceFilter === 'local' && queries().at(-1)?.folderId === null, 'Local filter clears folder scope');
  button($('#sources'), 'Tutte le sorgenti').click();
  await until(() => queries().at(-1)?.sourceFilter === 'all' && queries().at(-1)?.sourceId === null, 'All sources clears source scope');
  const toggle = $('#sources button[aria-expanded]');
  if (toggle.getAttribute('aria-expanded') === 'true') { toggle.click(); await wait(20); }
  assert.equal(toggle.getAttribute('aria-expanded'), 'false');
  assert.ok(!$('#sources').textContent.includes('Strumenti'));
  toggle.click(); await until(() => $('#sources').textContent.includes('Strumenti'), 'Expanding a source reveals its children');
  button($('#sources'), 'Strumenti').click();
  await until(() => $('#sources button[aria-current="location"]')?.textContent.includes('Strumenti'), 'Tree highlights selected folder');
});
