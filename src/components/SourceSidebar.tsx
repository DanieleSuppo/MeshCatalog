import { useEffect, useState } from 'react';
import { userMessage } from '../api/useCatalogRead';
import type { FolderTreeNode, SourceDto, SourceFilter } from '../types/catalog';
import { formatNumber, Icon, modelCountLabel } from './CatalogLabels';

export { userMessage as sourceErrorMessage };
export interface SourceSidebarProps {
  sources: SourceDto[];
  tree: FolderTreeNode[];
  activeFolderId: number | null;
  activeSourceId: number | null;
  activePath: FolderTreeNode[];
  filter: SourceFilter;
  onFilter: (filter: SourceFilter) => void;
  onNavigate: (sourceId: number, folderId: number | null) => void;
  actionError: string | null;
  busySourceId: number | null;
  onAddFolder: () => void;
  onScanSource: (sourceId: number) => void;
  onRemoveSource: (sourceId: number) => void;
  loading?: boolean;
  onRetry?: () => void;
}

type TreeProps = Pick<SourceSidebarProps, 'onNavigate' | 'activeFolderId' | 'activePath'>;
function Branch({ node, ...nav }: TreeProps & { node: FolderTreeNode }) {
  const inPath = nav.activePath.some(p => p.id === node.id);
  const [expanded, setExpanded] = useState(inPath);
  useEffect(() => { if (inPath) setExpanded(true); }, [inPath, nav.activeFolderId]);
  return <li>
    <div className="tree-row">
      {node.children.length > 0 ? <button className="tree-toggle" aria-expanded={expanded} aria-label={`${expanded ? 'Comprimi' : 'Espandi'} ${node.name}`} onClick={() => setExpanded(e => !e)}><Icon name="arrow" /></button> : <span className="tree-spacer" />}
      <button className="tree-select" aria-current={node.id === nav.activeFolderId ? 'location' : undefined} title={node.relPath || node.name}
        onClick={() => { nav.onNavigate(node.sourceId, node.id); setExpanded(true); }}>
        <Icon name="folder" /><span className="truncate">{node.name}</span><span className="tree-count" aria-label={modelCountLabel(node.recursiveModelCount)}>{formatNumber(node.recursiveModelCount)}</span>
      </button>
    </div>
    {expanded && node.children.length > 0 && <ul className="tree-children">{node.children.map(child => <Branch key={child.id} node={child} {...nav} />)}</ul>}
  </li>;
}

const states = { idle: 'Pronta', scanning: 'In scansione', unavailable: 'Non disponibile', error: 'Errore' };
function SourceEntry({ source, root, props }: { source: SourceDto; root?: FolderTreeNode; props: SourceSidebarProps }) {
  const busy = props.busySourceId === source.id;
  const [confirm, setConfirm] = useState(false);
  return <li className="source-entry">
    {root ? <ul><Branch node={root} activeFolderId={props.activeFolderId} activePath={props.activePath ?? []} onNavigate={props.onNavigate} /></ul>
      : <button className="tree-select" aria-current={props.activeSourceId === source.id ? 'location' : undefined} onClick={() => props.onNavigate(source.id, null)}><Icon name="folder" /><span className="truncate">{source.name}</span></button>}
    <details className="source-details">
      <summary data-status={source.status}>{states[source.status]} · {formatNumber(source.fileCount)} file</summary>
      <p className="break-path">{source.rootDisplayPath}</p>
      <p>{modelCountLabel(source.modelCount)} · {formatNumber(source.fileCount)} file</p>
      <p>{source.lastScanAt === null ? 'Nessuna scansione completata' : `Ultima scansione: ${new Date(source.lastScanAt * 1000).toLocaleString('it-IT')}`}</p>
      {confirm ? <div><p>Rimuovere la sorgente dal catalogo? I file sul disco restano intatti.</p><div className="action-row">
        <button disabled={busy} onClick={() => { setConfirm(false); props.onRemoveSource(source.id); }}>Conferma rimozione</button><button onClick={() => setConfirm(false)}>Annulla</button>
      </div></div> : <div className="action-row">
        <button disabled={busy || source.status === 'scanning'} onClick={() => props.onScanSource(source.id)} aria-label={`Scansiona ${source.name}`}>Scansiona</button>
        <button disabled={busy} onClick={() => setConfirm(true)} aria-label={`Rimuovi ${source.name}`}>Rimuovi</button>
      </div>}
    </details>
    {source.lastError && <p className="error-text" role="alert">{source.lastError}</p>}
  </li>;
}

export function SourceSidebar(props: SourceSidebarProps) {
  const { sources, tree = [], activeSourceId, filter, actionError } = props;
  return <aside id="sources" className="source-sidebar" aria-label="Sorgenti locali">
    <div className="panel-head"><h2>Sorgenti</h2><span className="number">{formatNumber(sources.length)}</span></div>
    <nav aria-label="Ambito del catalogo" className="scope-nav">{(['all', 'local'] as const).map(f => <button key={f}
      aria-current={filter === f && activeSourceId === null ? 'location' : undefined} onClick={() => props.onFilter(f)}>{f === 'all' ? 'Tutte le sorgenti' : 'Locali'}</button>)}</nav>
    <h3>Cartelle</h3>
    {actionError && <div className="notice" role="alert">{actionError}{props.onRetry && <button onClick={props.onRetry}>Riprova</button>}</div>}
    <nav aria-label="Cartelle delle sorgenti" className="tree-nav">
      {sources.length ? <ul>{sources.filter(s => filter !== 'local' || s.kind === 'local_dir').map(s => <SourceEntry key={s.id} source={s} root={tree.find(n => n.sourceId === s.id)} props={props} />)}</ul>
        : <p className="muted">{props.loading ? 'Caricamento sorgenti…' : actionError ? 'Sorgenti non disponibili.' : 'Nessuna sorgente registrata.'}</p>}
    </nav>
    <button className="add-source" onClick={props.onAddFolder} aria-label="Aggiungi cartella sorgente"><Icon name="add" />Aggiungi cartella</button>
  </aside>;
}
