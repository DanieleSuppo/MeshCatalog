import type { ContainerKind, StorageState } from '../types/catalog';

export const containers: Record<ContainerKind, string> = { folder_pack: 'Pack', zip_archive: 'Archivio ZIP', single_file: 'File singolo' };
export const storage: Record<StorageState, string> = { local_resident: 'Locale', cloud_placeholder: 'Cloud / Online-Only', zip_entry: 'Voce ZIP', dropbox_remote: 'Remoto' };
const number = new Intl.NumberFormat('it-IT', { maximumFractionDigits: 1 });
export const formatNumber = (value: number) => number.format(value);
export const modelCountLabel = (count: number) => `${formatNumber(count)} ${count === 1 ? 'modello' : 'modelli'}`;
export function formatBytes(bytes: number) {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  const scale = bytes > 0 ? Math.min(units.length - 1, Math.floor(Math.log2(bytes) / 10)) : 0;
  return `${formatNumber(bytes / 1024 ** scale)} ${units[scale]}`;
}

const paths = {
  catalog: 'M12 2 21 7v10l-9 5-9-5V7z M3 7l9 5 9-5 M12 12v10 M7.5 4.5l9 5',
  folder: 'M3 7V5a1 1 0 0 1 1-1h5l2 3h9a1 1 0 0 1 1 1v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z',
  search: 'M16.5 16.5 21 21 M18 10.5a7.5 7.5 0 1 1-15 0 7.5 7.5 0 0 1 15 0z',
  arrow: 'm9 5 7 7-7 7', close: 'm6 6 12 12 M18 6 6 18',
  scan: 'M4 10a8 8 0 0 1 14-4l2 2 M20 3v5h-5 M20 14a8 8 0 0 1-14 4l-2-2 M4 21v-5h5',
  info: 'M3 5h18v14H3z M13 5v14 M16 9h2 M16 12h2',
  add: 'M12 5v14 M5 12h14',
};
export function Icon({ name }: { name: keyof typeof paths }) {
  return <svg aria-hidden="true" focusable="false" className="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"><path d={paths[name]} /></svg>;
}
export const CloudBadge = () => <span className="cloud-badge">Cloud / Online-Only</span>;
