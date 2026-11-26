import type { FileCardDto, ModelCardDto } from '../types/catalog';
import { CloudBadge, containers, formatBytes, formatNumber, storage } from './CatalogLabels';

export function ModelOrFileCard({ item, selected, onInspect }: {
  item: FileCardDto | ModelCardDto; selected: boolean; onInspect: (id: number) => void;
}) {
  const model = 'containerKind' in item;
  const id = model ? item.id : item.modelId;
  const title = model ? item.displayTitle : item.filename;
  const path = model ? item.relativePath : item.relPath;
  const ext = model ? item.primaryExt : item.ext;
  const cloud = model ? item.hasCloudPlaceholder : item.storageState === 'cloud_placeholder';
  const content = <>
    <span className="card-line"><span className="format-badge">{ext?.toUpperCase() || 'PACK'}</span><span className="kind-label">{model ? containers[item.containerKind] : cloud ? 'File' : storage[item.storageState]}</span></span>
    <strong className="card-title truncate" title={title}>{title}</strong>
    <span className="card-path truncate" title={path}>{path || 'Radice sorgente'}</span>
    <span className="card-foot"><span>{model ? `${formatNumber(item.fileCount)} file · ` : ''}{formatBytes(model ? item.totalBytes : item.sizeBytes)}</span>{cloud && <CloudBadge />}</span>
  </>;
  return id === null ? <article className="catalog-card item-card" aria-label={title}>{content}</article>
    : <button type="button" className="catalog-card item-card" aria-controls="inspector" aria-pressed={selected} onClick={() => onInspect(id)}>{content}</button>;
}
