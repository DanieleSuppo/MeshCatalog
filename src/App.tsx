import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import * as ipc from './api/tauriClient';
import { useCatalogRead, userMessage } from './api/useCatalogRead';
import { CatalogGrid, EmptyCatalog } from './components/CatalogGrid';
import { formatNumber, Icon, modelCountLabel } from './components/CatalogLabels';
import { SourceSidebar } from './components/SourceSidebar';
import { TextInspectorPanel } from './components/TextInspectorPanel';
import { TopCommandBar } from './components/TopCommandBar';
import type { FolderTreeNode, IndexerProgressEvent, ScanStatusDto, SearchQuery, SourceFilter, ViewMode } from './types/catalog';

function pathTo(nodes: FolderTreeNode[], id: number | null): FolderTreeNode[] {
  if (id === null) return [];
  for (const node of nodes) {
    if (node.id === id) return [node];
    const trail = pathTo(node.children, id);
    if (trail.length) return [node, ...trail];
  }
  return [];
}

function useIndexer(refresh: (changedModels?: ReadonlySet<number>) => void) {
  const [status, setStatus] = useState<ScanStatusDto | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);
  const generation = useRef(0);
  const syncStatus = useCallback(async () => {
    const request = ++generation.current;
    try { const next = await ipc.get_scan_status(); if (generation.current === request) setStatus(next); }
    catch (e) { if (generation.current === request) setScanError(userMessage(e)); }
  }, []);
  useEffect(() => {
    let alive = true;
    let batchTimer: number | undefined, progressTimer: number | undefined;
    let latest: IndexerProgressEvent | undefined;
    let changedModels = new Set<number>();
    const disposers: (() => void)[] = [];
    const flushProgress = () => {
      progressTimer = undefined;
      if (!alive || !latest) return;
      const p = latest;
      setStatus(prev => ({ isRunning: true, activeSourceId: p.sourceId, pass: p.pass === 3 ? 2 : p.pass,
        phaseLabel: p.phaseLabel, processed: p.processed, total: p.total, currentItem: p.currentItem, recentErrors: prev?.recentErrors ?? [] }));
    };
    const subscriptions = [
      ipc.listenIndexer('indexer://progress', p => {
        if (!alive) return;
        ++generation.current; latest = p;
        if (progressTimer === undefined) progressTimer = window.setTimeout(flushProgress, 80);
      }),
      ipc.listenIndexer('indexer://batch_updated', batch => {
        if (!alive) return;
        for (const id of batch.updatedModelIds) changedModels.add(id);
        if (batchTimer === undefined) batchTimer = window.setTimeout(() => {
          batchTimer = undefined; refresh(changedModels); changedModels = new Set();
        }, 300);
      }),
      ipc.listenIndexer('indexer://error', p => {
        if (!alive) return;
        setScanError(`${p.recoverable ? 'Avviso' : 'Errore'}: ${p.message}${p.path ? ` · ${p.path}` : ''}`);
      }),
      ipc.listenIndexer('indexer://completed', () => {
        if (!alive) return;
        window.clearTimeout(batchTimer); window.clearTimeout(progressTimer);
        batchTimer = progressTimer = undefined;
        changedModels = new Set(); refresh(); void syncStatus();
      }),
    ];
    void Promise.all(subscriptions.map(sub => sub.then(dispose => { if (alive) disposers.push(dispose); else dispose(); })))
      .then(() => { if (alive) { void syncStatus(); refresh(); } })
      .catch(e => { if (alive) setScanError(userMessage(e)); });
    return () => { alive = false; ++generation.current; window.clearTimeout(batchTimer); window.clearTimeout(progressTimer); disposers.forEach(fn => fn()); };
  }, [refresh, syncStatus]);
  const scan = async (sourceId?: number) => {
    setScanError(null);
    try { await ipc.start_index_scan(sourceId); await syncStatus(); refresh(); } catch (e) { setScanError(userMessage(e)); }
  };
  const cancel = async () => { try { await ipc.cancel_index_scan(); await syncStatus(); } catch (e) { setScanError(userMessage(e)); } };
  return { status, scanError, scan, cancel };
}

export function App() {
  const [revision, setRevision] = useState(0);
  const [selection, select] = useState<{ modelId: number; itemId: number; revision: number } | null>(null);
  const refresh = useCallback((changedModels?: ReadonlySet<number>) => {
    setRevision(v => v + 1);
    select(s => s && (!changedModels || changedModels.has(s.modelId)) ? { ...s, revision: s.revision + 1 } : s);
  }, []);
  const indexer = useIndexer(refresh);
  const [activeSourceFilter, setFilter] = useState<SourceFilter>('all');
  const [activeSourceId, setSource] = useState<number | null>(null);
  const [activeFolderId, setFolder] = useState<number | null>(null);
  const [viewMode, setView] = useState<ViewMode>('models');
  const [searchText, setSearchText] = useState('');
  const [text, setText] = useState('');
  const [navOpen, setNavOpen] = useState(false);
  const [busy, setBusy] = useState<number | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const sources = useCatalogRead(ipc.list_sources, revision);
  const loadTree = useCallback(() => ipc.get_folder_tree(activeSourceFilter), [activeSourceFilter]);
  const tree = useCatalogRead(loadTree, revision);
  const trail = useMemo(() => pathTo(tree.value ?? [], activeFolderId), [tree.value, activeFolderId]);
  const source = sources.value?.find(s => s.id === activeSourceId);
  const scope = trail.at(-1)?.name ?? source?.name ?? (activeSourceFilter === 'local' ? 'Sorgenti locali' : 'Tutte le sorgenti');

  const navigate = (sourceId: number | null, folderId: number | null) => {
    setSource(sourceId); setFolder(folderId); select(null); setNavOpen(false); heading.current?.focus({ preventScroll: true });
  };
  const changeFilter = (filter: SourceFilter) => { setFilter(filter); navigate(null, null); };
  useEffect(() => {
    if (activeSourceId === null || !tree.value || !sources.value) return;
    if (!source) { setSource(null); setFolder(null); select(null); return; }
    // Pass 2 may absorb a provisional folder. Stay inside the source instead of showing a dead scope.
    if (!trail.length) {
      const rootId = tree.value.find(n => n.sourceId === activeSourceId)?.id ?? null;
      if (rootId !== activeFolderId) { setFolder(rootId); select(null); }
    }
  }, [activeSourceId, activeFolderId, tree.value, sources.value, source, trail]);
  useEffect(() => {
    const timer = window.setTimeout(() => setText(searchText.trim()), 45);
    return () => window.clearTimeout(timer);
  }, [searchText]);
  const query: SearchQuery = useMemo(() => ({ text, viewMode, sourceFilter: activeSourceFilter, sourceId: activeSourceId,
    folderId: activeFolderId, offset: 0, limit: 96, facets: {} }), [text, viewMode, activeSourceFilter, activeSourceId, activeFolderId]);
  const load = useCallback(() => ipc.search_catalog(query), [query]);
  const catalog = useCatalogRead(load, revision);
  const page = catalog.value;
  const addFolder = async () => {
    setActionError(null);
    try {
      const picked = await ipc.pick_local_folder();
      if (picked !== null) { const id = await ipc.add_local_source(picked); refresh(); await indexer.scan(id); }
    } catch (e) { setActionError(userMessage(e)); setNavOpen(true); }
  };
  const removeSource = async (id: number) => {
    setBusy(id); setActionError(null);
    try { await ipc.remove_source(id); refresh(); } catch (e) { setActionError(userMessage(e)); } finally { setBusy(null); }
  };
  const inspect = (modelId: number, itemId: number) => {
    opener.current = document.activeElement as HTMLElement;
    select({ modelId, itemId, revision: 0 });
  };
  const closeInspector = () => { select(null); (opener.current?.isConnected ? opener.current : heading.current)?.focus(); };
  const firstRun = sources.value?.length === 0;
  const hasContent = page && (page.subfolders.length > 0 || page.items.length > 0);
  return <div className="studio-shell" data-nav-open={navOpen}>
    <TopCommandBar status={indexer.status} scanError={indexer.scanError} onScanAll={() => void indexer.scan()} onCancelScan={indexer.cancel}
      viewMode={viewMode} onView={mode => { setView(mode); select(null); }} searchText={searchText} onSearch={value => { setSearchText(value); select(null); }}
      queryTimeMs={page?.queryTimeMs} hasSources={!!sources.value?.length} navOpen={navOpen} onToggleNav={() => setNavOpen(open => !open)} />
    <SourceSidebar sources={sources.value ?? []} tree={tree.value ?? []} activeFolderId={activeFolderId} activeSourceId={activeSourceId} activePath={trail}
      filter={activeSourceFilter} onFilter={changeFilter} onNavigate={navigate} actionError={actionError || sources.error || tree.error || null}
      busySourceId={busy ?? (indexer.status?.activeSourceId ?? null)} onAddFolder={addFolder} onRemoveSource={removeSource} onScanSource={indexer.scan}
      loading={!sources.value && !sources.error} onRetry={sources.error || tree.error ? () => refresh() : undefined} />
    <main className="catalog-main">
      <div className="catalog-heading">
        <nav className="breadcrumbs" aria-label="Percorso del catalogo"><button onClick={() => changeFilter(activeSourceFilter)}>Catalogo</button>{trail.slice(0, -1).map(n => <span key={n.id}><Icon name="arrow" /><button onClick={() => navigate(n.sourceId, n.id)}>{n.name}</button></span>)}</nav>
        <h1 ref={heading} tabIndex={-1}>{scope}</h1>
        <p role="status">{page ? `${modelCountLabel(page.totalModels)} · ${formatNumber(page.totalFiles)} file${text ? ' nella ricerca' : ''}` : catalog.error ? 'Conteggi non disponibili' : 'Lettura del catalogo…'}</p>
      </div>
      {catalog.error && <div className="notice" role="alert">{catalog.error} <button onClick={catalog.retry}>Riprova</button></div>}
      {hasContent ? <CatalogGrid query={query} firstPage={page} selectedItemId={selection?.itemId ?? null} onOpen={f => navigate(f.sourceId, f.id)} onInspect={inspect} />
        : page && !catalog.error ? <EmptyCatalog title={text ? 'Nessun risultato' : firstRun ? 'Il catalogo è vuoto' : 'Nessun elemento indicizzato'}>
          <p>{text ? 'Prova un altro titolo, percorso o formato.' : firstRun ? 'Aggiungi la prima cartella dei modelli. I file originali restano sul disco, in sola lettura.' : 'Avvia una scansione o scegli un’altra cartella.'}</p>
          {text ? <button onClick={() => setSearchText('')}>Cancella ricerca</button> : firstRun ? <button onClick={addFolder}>Aggiungi cartella</button> : null}
        </EmptyCatalog> : null}
    </main>
    <TextInspectorPanel modelId={selection?.modelId ?? null} revision={selection?.revision ?? 0} onClose={closeInspector} />
  </div>;
}
