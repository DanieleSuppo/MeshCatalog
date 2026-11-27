import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { createServer } from 'vite';

let server;
let client;
const calls = [];
let callback;

before(async () => {
  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke: async (command, args) => {
        calls.push([command, args]);
        if (command === 'plugin:event|listen') return 42;
        if (command === 'list_sources') return [{ id: 1 }];
      },
      transformCallback: (handler) => {
        callback = handler;
        return 7;
      },
    },
    __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener() {} },
  };
  server = await createServer({ server: { middlewareMode: true, ws: false }, appType: 'custom' });
  client = await server.ssrLoadModule('/src/api/tauriClient.ts');
});

after(async () => {
  await server?.close();
  delete globalThis.window;
});

test('catalog commands use the Tauri names and camelCase parameters', async () => {
  const query = { text: '', viewMode: 'models', sourceFilter: 'all', folderId: null, facets: {}, limit: 10, offset: 0 };
  assert.deepEqual(await client.list_sources(), [{ id: 1 }]);
  await client.add_local_source('D:/Pack');
  await client.add_local_source('D:/Pack', 'Pack');
  await client.remove_source(1);
  await client.pick_local_folder();
  await client.start_index_scan();
  await client.start_index_scan(1);
  await client.cancel_index_scan();
  await client.cancel_index_scan(1);
  await client.get_scan_status();
  await client.get_folder_tree('local');
  await client.search_catalog(query);
  await client.get_model_detail(2);

  assert.deepEqual(calls.splice(0), [
    ['list_sources', {}], ['add_local_source', { path: 'D:/Pack' }],
    ['add_local_source', { path: 'D:/Pack', name: 'Pack' }], ['remove_source', { sourceId: 1 }],
    ['pick_local_folder', {}], ['start_index_scan', {}], ['start_index_scan', { sourceId: 1 }],
    ['cancel_index_scan', {}], ['cancel_index_scan', { sourceId: 1 }], ['get_scan_status', {}],
    ['get_folder_tree', { sourceFilter: 'local' }], ['search_catalog', { query }], ['get_model_detail', { modelId: 2 }],
  ]);
});

test('indexer listener forwards payload and returns the unsubscribe function', async () => {
  const payload = { sourceId: 1, updatedFolderIds: [2], updatedModelIds: [3] };
  const received = [];
  const unlisten = await client.listenIndexer('indexer://batch_updated', (data) => received.push(data));
  callback({ event: 'indexer://batch_updated', id: 42, payload });
  assert.deepEqual(received, [payload]);
  await unlisten();
  assert.deepEqual(calls.splice(0), [
    ['plugin:event|listen', { event: 'indexer://batch_updated', target: { kind: 'Any' }, handler: 7 }],
    ['plugin:event|unlisten', { event: 'indexer://batch_updated', eventId: 42 }],
  ]);
});
