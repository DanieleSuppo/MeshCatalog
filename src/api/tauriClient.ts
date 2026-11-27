import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { UnlistenFn } from '@tauri-apps/api/event';
import type {
  FolderTreeNode,
  IndexerEvents,
  ModelDetailDto,
  ScanStatusDto,
  SearchQuery,
  SearchResultPage,
  SourceDto,
  SourceFilter,
} from '../types/catalog';

export const list_sources = () => invoke<SourceDto[]>('list_sources');
export const add_local_source = (path: string, name?: string) =>
  invoke<number>('add_local_source', name === undefined ? { path } : { path, name });
export const remove_source = (sourceId: number) => invoke<void>('remove_source', { sourceId });
export const pick_local_folder = () => invoke<string | null>('pick_local_folder');
export const start_index_scan = (sourceId?: number) =>
  invoke<void>('start_index_scan', sourceId === undefined ? undefined : { sourceId });
export const cancel_index_scan = (sourceId?: number) =>
  invoke<void>('cancel_index_scan', sourceId === undefined ? undefined : { sourceId });
export const get_scan_status = () => invoke<ScanStatusDto>('get_scan_status');
export const get_folder_tree = (sourceFilter: SourceFilter) =>
  invoke<FolderTreeNode[]>('get_folder_tree', { sourceFilter });
export const search_catalog = (query: SearchQuery) => invoke<SearchResultPage>('search_catalog', { query });
export const get_model_detail = (modelId: number) => invoke<ModelDetailDto>('get_model_detail', { modelId });

export function listenIndexer<Name extends keyof IndexerEvents>(
  name: Name,
  onPayload: (payload: IndexerEvents[Name]) => void,
): Promise<UnlistenFn> {
  return listen<IndexerEvents[Name]>(name, ({ payload }) => onPayload(payload));
}
