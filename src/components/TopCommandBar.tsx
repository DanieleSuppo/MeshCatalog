import type { ScanStatusDto, ViewMode } from '../types/catalog';
import { formatNumber, Icon } from './CatalogLabels';

export interface TopCommandBarProps {
  status: ScanStatusDto | null;
  scanError?: string | null;
  onScanAll: () => void;
  onCancelScan: () => void;
  viewMode: ViewMode;
  onView: (mode: ViewMode) => void;
  searchText: string;
  onSearch: (text: string) => void;
  queryTimeMs?: number;
  hasSources: boolean;
  navOpen: boolean;
  onToggleNav: () => void;
}

export function TopCommandBar({ status, scanError, onScanAll, onCancelScan, viewMode, onView, searchText, onSearch,
  queryTimeMs, hasSources, navOpen, onToggleNav }: TopCommandBarProps) {
  const running = status?.isRunning;
  const lastIssue = status?.recentErrors.at(-1);
  const phase = status?.phaseLabel || 'Inventario';
  return <header className="command-area">
    <div className="top-command-bar">
      <div className="brand"><Icon name="catalog" /><span>MeshCatalog</span></div>
      <button className="nav-trigger" aria-controls="sources" aria-expanded={navOpen} onClick={onToggleNav}>Sorgenti</button>
      <label className="search-control"><Icon name="search" /><input type="search" aria-label="Cerca nel catalogo" placeholder="Cerca titoli, percorsi o formati…" value={searchText ?? ''} onChange={e => onSearch(e.target.value)} /></label>
      <div className="view-switch" role="group" aria-label="Vista del catalogo">{(['models', 'files'] as const).map(mode => <button key={mode} type="button" aria-pressed={viewMode === mode} onClick={() => onView(mode)}>{mode === 'models' ? 'Modelli / Pack' : 'Singoli File'}</button>)}</div>
      <button className="scan-action" onClick={onScanAll} disabled={running || !hasSources}><Icon name="scan" />Scansiona tutto</button>
    </div>
    <div className="index-status">
      <span className={`status-dot${running ? ' running' : ''}`} aria-hidden="true" />
      <span className="scan-label" role="status">{running ? (/^passata/i.test(phase) ? phase : `Passata ${status.pass ?? 1} — ${phase}`) : status ? 'Pronto' : 'Stato scansione non disponibile'}</span>
      {running && <><span className="number">{formatNumber(status.processed)}{status.total > 0 ? ` / ${formatNumber(status.total)}` : ' elaborati'}</span>
        <progress role="progressbar" aria-label="Avanzamento scansione" value={status.total ? status.processed : undefined} max={status.total || undefined} />
        {status.currentItem && <span className="current-item truncate" title={status.currentItem}>{status.currentItem}</span>}
        <button onClick={onCancelScan} aria-label="Annulla scansione">Annulla</button></>}
      <span className="query-time">Query <span className="number">{queryTimeMs == null ? '—' : `${formatNumber(queryTimeMs)} ms`}</span></span>
    </div>
    {(scanError || lastIssue) && <div className="scan-notice" role="status">{scanError || `${lastIssue!.recoverable ? 'Avviso' : 'Errore'}: ${lastIssue!.message}${lastIssue!.path ? ` · ${lastIssue!.path}` : ''}`}</div>}
  </header>;
}
