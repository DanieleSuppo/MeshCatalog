import type { FolderCardDto } from '../types/catalog';
import { CloudBadge, formatNumber, Icon, modelCountLabel } from './CatalogLabels';

export function FolderCard({ folder, onOpen }: { folder: FolderCardDto; onOpen: (folder: FolderCardDto) => void }) {
  return <button type="button" className="catalog-card folder-card" onClick={() => onOpen(folder)}>
    <span className="card-line"><Icon name="folder" /><strong className="truncate" title={folder.name}>{folder.name}</strong><Icon name="arrow" /></span>
    <span className="card-counts">{modelCountLabel(folder.recursiveModelCount)} · {formatNumber(folder.recursiveFileCount)} file</span>
    <span className="sample-titles">{folder.sampleTitles.slice(0, 4).map((title, i) => <span key={i} className="truncate" title={title}>{title}</span>)}</span>
    <span className="card-foot"><span>{formatNumber(folder.nestedFolderCount)} {folder.nestedFolderCount === 1 ? 'sottocartella' : 'sottocartelle'}</span>{folder.hasCloudPlaceholder && <CloudBadge />}</span>
  </button>;
}
