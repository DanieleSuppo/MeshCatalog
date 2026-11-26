import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { createServer } from 'vite';
import { renderToStaticMarkup } from 'react-dom/server';

let server;
let SourceSidebar;
let TopCommandBar;
let sourceErrorMessage;

test.before(async () => {
  server = await createServer({ server: { middlewareMode: true, ws: false }, appType: 'custom' });
  ({ SourceSidebar, sourceErrorMessage } = await server.ssrLoadModule('/src/components/SourceSidebar.tsx'));
  ({ TopCommandBar } = await server.ssrLoadModule('/src/components/TopCommandBar.tsx'));
});

test.after(async () => {
  await server?.close();
});

const source = {
  id: 1,
  kind: 'local_dir',
  name: 'Stampa3D',
  rootUri: 'file:///D:/Stampa3D',
  rootDisplayPath: 'D:\\Stampa3D',
  status: 'idle',
  lastError: null,
  lastScanAt: null,
  modelCount: 120,
  fileCount: 340,
};

const noop = () => {};

test('T022 sidebar renders add action, source status and counts', () => {
  const html = renderToStaticMarkup(
    SourceSidebar({ sources: [source], actionError: null, busySourceId: null, onAddFolder: noop, onScanSource: noop, onRemoveSource: noop }),
  );
  assert.match(html, /Aggiungi cartella/);
  assert.match(html, /Stampa3D/);
  assert.match(html, /idle/);
  assert.match(html, /120/);
  assert.match(html, /340/);
  assert.match(html, /Scansiona/);
  assert.match(html, /Rimuovi/);
});

test('T022 sidebar surfaces lastError and action errors as alerts', () => {
  const withLastError = renderToStaticMarkup(
    SourceSidebar({
      sources: [{ ...source, status: 'error', lastError: 'disco scollegato' }],
      actionError: null,
      busySourceId: null,
      onAddFolder: noop,
      onScanSource: noop,
      onRemoveSource: noop,
    }),
  );
  assert.match(withLastError, /disco scollegato/);
  assert.match(withLastError, /role="alert"/);

  const withActionError = renderToStaticMarkup(
    SourceSidebar({ sources: [], actionError: 'Cartella già registrata', busySourceId: null, onAddFolder: noop, onScanSource: noop, onRemoveSource: noop }),
  );
  assert.match(withActionError, /Cartella già registrata/);
  assert.match(withActionError, /role="alert"/);
});

test('T022 sidebar maps overlap and invalid path codes to user-facing messages', () => {
  assert.match(sourceErrorMessage({ code: 'SOURCE_OVERLAP', message: 'overlap' }), /già registrata|sovrapp/i);
  assert.match(sourceErrorMessage({ code: 'INVALID_PATH', message: 'bad' }), /non valido|leggibile/i);
  assert.match(sourceErrorMessage({ code: 'SCAN_ALREADY_RUNNING', message: 'busy' }), /già in corso/i);
  assert.equal(sourceErrorMessage({ code: 'INTERNAL_ERROR', message: 'boom' }), 'boom');
});

test('T022 topbar renders scan-all action and idle pill', () => {
  const html = renderToStaticMarkup(
    TopCommandBar({
      status: { isRunning: false, activeSourceId: null, pass: null, phaseLabel: null, processed: 0, total: 0, currentItem: null, recentErrors: [] },
      onScanAll: noop,
      onCancelScan: noop,
    }),
  );
  assert.match(html, /Scansiona tutto/);
  assert.match(html, /Pronto/);
});

test('T022 topbar renders running progress pill with cancel', () => {
  const html = renderToStaticMarkup(
    TopCommandBar({
      status: { isRunning: true, activeSourceId: 1, pass: 1, phaseLabel: 'Inventario cartelle', processed: 40, total: 100, currentItem: 'D:\\Stampa3D', recentErrors: [] },
      onScanAll: noop,
      onCancelScan: noop,
    }),
  );
  assert.match(html, /Passata 1/);
  assert.match(html, /Inventario cartelle/);
  assert.match(html, /40/);
  assert.match(html, /Annulla/);
  assert.match(html, /role="progressbar"/);
});

test('T022 components keep project tokens and stay WebGL-free', async () => {
  for (const file of ['src/components/SourceSidebar.tsx', 'src/components/TopCommandBar.tsx']) {
    const code = await readFile(file, 'utf8');
    assert.doesNotMatch(code, /three|webgl|@react-three/i);
    assert.doesNotMatch(code, /#[0-9a-fA-F]{3,8}/);
  }
  const css = await readFile('src/styles/theme.css', 'utf8');
  for (const token of ['--studio-background', '--studio-surface', '--studio-border']) {
    assert.ok(css.includes(token), `missing token ${token}`);
  }
});
