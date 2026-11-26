// Synthetic IPC boundary for UI tests. No fixture refers to a real directory.
export const source = {
  id: 1, kind: 'local_dir', name: 'Atelier', rootDisplayPath: 'D:\\Atelier', rootUri: 'local://D:/Atelier',
  status: 'idle', lastError: null, lastScanAt: 1600000000, modelCount: 50010, fileCount: 50011,
};
const samples = ['Astrolabio', 'Meridiana', 'Quadrante', 'Sfera'];
const folder = (id, name, parentId, children = []) => ({
  id, sourceId: 1, name, parentId, relPath: parentId === null ? '' : name, children,
  recursiveModelCount: 50010, recursiveFileCount: 50011, nestedFolderCount: children.length,
  sampleTitles: samples, hasCloudPlaceholder: true,
});
const section = folder(12, 'Strumenti', 10);
const root = folder(10, 'Atelier', null, [section]);
const model = (index) => ({
  id: 20 + index, sourceId: 1, folderId: 12, canonicalUri: `local://D:/Atelier/Strumenti/Astro_${index}.zip`,
  displayTitle: `Astrolabio ${index}`, rawName: `Astro_${index}.zip`, containerKind: 'zip_archive',
  primaryExt: 'zip', hasCloudPlaceholder: false, fileCount: 2, totalBytes: 3072,
  relativePath: `Strumenti/Astro_${index}.zip`, updatedAt: 0,
});
const file = (id, relPath, modelId) => ({ id, sourceId: 1, folderId: 12, modelId,
  relPath, filename: relPath.split('/').at(-1), ext: relPath.split('.').at(-1),
  uri: `local://D:/Atelier/${relPath}`, parentArchiveId: null, parentArchiveName: null,
  mtimeUnix: 0, sizeBytes: 1024, storageState: modelId === null ? 'cloud_placeholder' : 'local_resident',
});
export const idle = { isRunning: false, pass: null, currentItem: null, activeSourceId: null,
  phaseLabel: null, processed: 0, total: 0, recentErrors: [] };
export function catalogFixture(command, args) {
  switch (command) {
    case 'list_sources': return [source];
    case 'get_folder_tree': return [root];
    case 'get_scan_status': return idle;
    case 'get_model_detail': return { ...model(args.modelId - 20), sourceName: 'Atelier',
      files: [file(90, 'Astro_0.zip', args.modelId), { ...file(91, 'interno/disco.stl', args.modelId), storageState: 'zip_entry' }] };
    case 'search_catalog': {
      const { folderId, text, viewMode, limit, offset } = args.query;
      const rootBrowse = !text && folderId === null;
      const files = [file(99, 'Strumenti/Astrolabio.png', null), file(90, 'Strumenti/Astro_0.zip', 20)];
      return { totalModels: 50010, totalFiles: rootBrowse ? 50011 : files.length,
        subfolders: text ? [] : rootBrowse ? [root] : folderId === 10 ? [section] : [],
        items: rootBrowse ? [] : viewMode === 'files' ? files.slice(offset, offset + limit)
          : Array.from({ length: Math.min(limit, 50010 - offset) }, (_, i) => model(offset + i)),
        queryTimeMs: 7.4, facetCounts: {} };
    }
    case 'cancel_index_scan': case 'start_index_scan': return;
    default: throw new Error(`Unexpected IPC command: ${command}`);
  }
}
