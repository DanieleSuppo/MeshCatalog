import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { JSDOM } from 'jsdom';
import { createServer } from 'vite';
import { mockIPC } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import { catalogFixture } from './fixtures/catalog.mjs';

let server, dom;
const calls = [];
let renamed = false;
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
before(async () => {
  dom = new JSDOM('<div id="root"></div>', { pretendToBeVisual: true, url: 'http://localhost' });
  globalThis.window = dom.window; globalThis.document = window.document;
  globalThis.ResizeObserver = window.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} };
  for (const [prop, value] of Object.entries({ clientWidth: 680, clientHeight: 620 })) {
    Object.defineProperty(window.HTMLElement.prototype, prop, { get: () => value });
  }
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    const result = catalogFixture(cmd, args);
    if (renamed && cmd === 'search_catalog' && args.query.offset > 0) {
      return { ...result, items: result.items.map(item => ({ ...item, displayTitle: 'Titolo aggiornato' })) };
    }
    return result;
  }, { shouldMockEvents: true });
  server = await createServer({ server: { middlewareMode: true, ws: false }, appType: 'custom' });
  await server.ssrLoadModule('/src/main.tsx');
});
after(async () => { await server?.close(); dom?.window.close(); });

test('scroll resets on criteria change, never on refresh/pagination', async () => {
  await until(() => $('main')?.textContent.includes('Quadrante'), 'Root folders render');
  button($('main'), 'Atelier').click();
  await until(() => queries().some(q => q.folderId === 10), 'Scope Atelier folder');
  button($('main'), 'Strumenti').click();
  await until(() => queries().at(-1)?.folderId === 12, 'Scope nested folder');
  const scroll = await until(() => $('[aria-label="Risultati del catalogo"]'), 'Scrollable viewport exists');
  // Paginate deep: real scrollTop drives later SQLite pages.
  scroll.scrollTop = 27000; scroll.dispatchEvent(new window.Event('scroll'));
  const deepQuery = await until(() => queries().find(q => q.folderId === 12 && q.offset > 0), 'Deep scroll fetches later pages');
  await until(() => /Astrolabio \d+/.test(scroll.textContent), 'Deep page renders before refresh');
  const position = scroll.scrollTop;
  assert.ok(position > 0, 'Setup scrolled away from top');
  // Refresh (committed batch, same criteria) must update the deep page without moving the viewport.
  const pagesBefore = queries().length;
  renamed = true;
  await emit('indexer://batch_updated', { sourceId: 1, updatedFolderIds: [12], updatedModelIds: [21] });
  await until(() => queries().slice(pagesBefore).some(q => q.folderId === 12 && q.offset === deepQuery.offset), 'Refresh re-queries the deep page');
  await until(() => scroll.textContent.includes('Titolo aggiornato'), 'Deep page displays the refreshed title');
  assert.equal($('[aria-label="Risultati del catalogo"]'), scroll, 'Refresh must not remount the viewport');
  assert.equal(scroll.scrollTop, position, 'Refresh must preserve scroll position');
  // Criteria change (view mode) must reset scroll to top (viewport remounts on new scope).
  button(document, 'Singoli File').click();
  await until(() => queries().at(-1)?.viewMode === 'files', 'View mode switched');
  const rescrolled = await until(() => $('[aria-label="Risultati del catalogo"]'), 'Viewport present after criteria change');
  await until(() => rescrolled.scrollTop === 0, 'View-mode change resets scroll to top');
});
