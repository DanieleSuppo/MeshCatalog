import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { search_catalog } from '../api/tauriClient';
import { useCatalogRead } from '../api/useCatalogRead';
import type { FolderCardDto, SearchQuery, SearchResultPage } from '../types/catalog';
import { formatNumber, Icon } from './CatalogLabels';
import { FolderCard } from './FolderCard';
import { ModelOrFileCard } from './ModelOrFileCard';

interface GridProps {
  query: SearchQuery;
  firstPage: SearchResultPage;
  revision: number;
  onOpen: (folder: FolderCardDto) => void;
  onInspect: (modelId: number, itemId: number) => void;
  selectedItemId: number | null;
}

// Folder roots have recursive totals but deliberately return no direct items.
export function CatalogGrid({ query, firstPage, revision, onOpen, onInspect, selectedItemId }: GridProps) {
  const viewport = useRef<HTMLDivElement>(null);
  const [columns, setColumns] = useState(1);
  useEffect(() => {
    const node = viewport.current!;
    const resize = () => setColumns(Math.max(1, Math.floor((node.clientWidth + 10) / 250)));
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  const folders = firstPage.subfolders;
  const count = !query.text && query.folderId === null ? 0 : query.viewMode === 'models' ? firstPage.totalModels : firstPage.totalFiles;
  const folderRows = Math.ceil(folders.length / columns);
  const itemHeader = folderRows ? folderRows + 1 : 0;
  const firstItemRow = itemHeader + 1;
  const rowCount = itemHeader + (count ? 1 + Math.ceil(count / columns) : 0);
  const rowHeight = (i: number) => i === 0 || i === itemHeader ? 42 : i < itemHeader ? 220 : 158;
  const rows = useVirtualizer({ count: rowCount, getScrollElement: () => viewport.current, estimateSize: rowHeight, overscan: 2 });
  // Reset scroll only when navigation criteria change (folder, source/filter, view mode, text).
  // Refresh (revision/firstPage) and pagination (offset pages) must keep the position.
  const criteriaKey = `${query.text}\0${query.viewMode}\0${query.sourceFilter}\0${query.sourceId ?? ''}\0${query.folderId ?? ''}`;
  useEffect(() => {
    viewport.current?.scrollTo?.({ top: 0 });
    // scrollTo may be unavailable in test DOM; scrollTop assignment is the observable contract.
    if (viewport.current) viewport.current.scrollTop = 0;
    rows.scrollToIndex(0, { align: 'start' });
  }, [criteriaKey, rows]);
  // Some row indices change kind (heading / folder / item) when the width or inventory changes.
  useEffect(() => rows.measure(), [rows, columns, itemHeader]);
  const visible = rows.getVirtualItems();
  const first = Math.max(0, (visible[0]?.index ?? 0) - firstItemRow) * columns;
  const last = Math.min(count - 1, ((visible.at(-1)?.index ?? 0) - firstItemRow + 1) * columns - 1);
  const startPage = Math.max(1, Math.floor(first / query.limit));
  const endPage = Math.floor(last / query.limit);
  type Items = SearchResultPage['items'];
  // A committed batch invalidates cached pages, not the scroll position.
  const cache = useMemo(() => new Map<number, { request: Promise<Items>; items?: Items }>(), [query, revision]);
  const loadWindow = useCallback(async () => {
    // Keep only intersecting pages. Reuse the shared edge (including an in-flight read) when scrolling.
    for (const p of cache.keys()) if (p < startPage || p > endPage) cache.delete(p);
    await Promise.all(Array.from({ length: Math.max(0, endPage - startPage + 1) }, async (_, i) => {
      const p = startPage + i;
      let entry = cache.get(p);
      if (!entry) {
        entry = { request: search_catalog({ ...query, offset: p * query.limit }).then(result => result.items) };
        cache.set(p, entry);
      }
      try { entry.items = await entry.request; }
      catch (error) { if (cache.get(p) === entry) cache.delete(p); throw error; }
    }));
  }, [query, startPage, endPage, cache]);
  const window = useCatalogRead(loadWindow, 0);
  const pageAt = (i: number) => i < query.limit ? firstPage.items[i] : cache.get(Math.floor(i / query.limit))?.items?.[i % query.limit];

  return <>
    {window.error && <div role="alert" className="notice">{window.error} <button onClick={window.retry}>Riprova</button></div>}
    <div className="grid-viewport" ref={viewport} tabIndex={0} role="region" aria-label="Risultati del catalogo">
      <div className="virtual-space" style={{ height: rows.getTotalSize() }}>
        {visible.map(({ index, start, key, size }) => {
          const folderHeader = folders.length > 0 && index === 0;
          const heading = folderHeader || index === itemHeader;
          const isFolder = index < itemHeader;
          const from = (index - (isFolder ? 1 : firstItemRow)) * columns;
          return <div key={key} className={`virtual-row${heading ? ' section-row' : ''}`} style={{ height: size, transform: `translateY(${start}px)`, gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>
            {heading ? <h2>{folderHeader ? (query.folderId === null ? 'Cartelle sorgente' : 'Sottocartelle') : query.text ? 'Risultati della ricerca' : query.viewMode === 'models' ? 'Modelli / Pack' : 'Singoli file'} <span className="number">{formatNumber(folderHeader ? folders.length : count)}</span></h2>
              : isFolder ? folders.slice(from, from + columns).map(folder => <FolderCard key={folder.id} folder={folder} onOpen={onOpen} />)
              : Array.from({ length: Math.min(columns, count - from) }, (_, j) => {
                const item = pageAt(from + j);
                return item ? <ModelOrFileCard key={item.id} item={item} selected={selectedItemId === item.id} onInspect={id => onInspect(id, item.id)} />
                  : <div key={`pending-${j}`} className="catalog-card pending" aria-label="Caricamento elemento" />;
              })}
          </div>;
        })}
      </div>
    </div>
  </>;
}

export function EmptyCatalog({ title, children }: { title: string; children: React.ReactNode }) {
  return <section className="empty-catalog"><Icon name="folder" /><h2>{title}</h2>{children}</section>;
}
