import { useCallback, useEffect, useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { get_model_detail } from '../api/tauriClient';
import { useCatalogRead } from '../api/useCatalogRead';
import type { FileCardDto } from '../types/catalog';
import { CloudBadge, containers, formatBytes, formatNumber, Icon, storage } from './CatalogLabels';

function Files({ files }: { files: FileCardDto[] }) {
  const list = useRef<HTMLDivElement>(null);
  const rows = useVirtualizer({ count: files.length, getScrollElement: () => list.current, estimateSize: () => 64, overscan: 2 });
  return <div className="inspector-files" ref={list} tabIndex={0} role="region" aria-label="Elenco dei file contenuti">
    <ol className="virtual-space" style={{ height: rows.getTotalSize() }}>{rows.getVirtualItems().map(row => {
      const file = files[row.index];
      return <li key={file.id} ref={rows.measureElement} data-index={row.index} aria-posinset={row.index + 1} aria-setsize={files.length} style={{ transform: `translateY(${row.start}px)` }}>
        <span className="break-path">{file.relPath}</span><small>{file.ext.toUpperCase()} · {formatBytes(file.sizeBytes)} · {storage[file.storageState]}</small>
      </li>;
    })}</ol>
  </div>;
}

function Detail({ modelId, revision }: { modelId: number; revision: number }) {
  const load = useCallback(() => get_model_detail(modelId), [modelId]);
  const { value: detail, error, retry } = useCatalogRead(load, revision);
  if (error) return <div role="alert" className="notice">{error} <button onClick={retry}>Riprova</button></div>;
  if (!detail) return <p className="muted panel-inset" role="status">Caricamento dettagli…</p>;
  return <>
    <div className="detail-summary">
      <span className="format-badge">{detail.primaryExt?.toUpperCase() || 'PACK'}</span>
      <h3>{detail.displayTitle}</h3>
      <dl>{[['Nome originale', detail.rawName], ['Percorso canonico', detail.canonicalUri], ['Contenitore', containers[detail.containerKind]], ['Dimensione', formatBytes(detail.totalBytes)]].map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>
      {detail.hasCloudPlaceholder && <CloudBadge />}
    </div>
    <h3 className="file-heading">File nel modello <span className="number">{formatNumber(detail.fileCount)}</span></h3>
    {detail.files.length ? <Files files={detail.files} /> : <p className="panel-inset muted">Nessun file indicizzato.</p>}
  </>;
}

export function TextInspectorPanel({ modelId, revision, onClose }: { modelId: number | null; revision: number; onClose: () => void }) {
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { if (modelId !== null) heading.current?.focus({ preventScroll: true }); }, [modelId]);
  return <aside id="inspector" className="inspector" data-selected={modelId !== null} aria-labelledby="inspector-title" onKeyDown={e => { if (e.key === 'Escape' && modelId !== null) onClose(); }}>
    <div className="panel-head"><h2 id="inspector-title" ref={heading} tabIndex={-1}>Inspector</h2>
      {modelId !== null && <button className="icon-button" aria-label="Chiudi Inspector" onClick={onClose}><Icon name="close" /></button>}</div>
    {modelId === null ? <div className="inspector-empty"><Icon name="info" /><h3>Nessuna selezione</h3><p>Seleziona un modello per leggerne i dettagli e l’elenco dei file.</p></div> : <Detail key={modelId} modelId={modelId} revision={revision} />}
  </aside>;
}
