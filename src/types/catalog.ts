export type ViewMode = 'models' | 'files';
export type SourceFilter = 'all' | 'local' | 'dropbox';
export type SourceKind = 'local_dir' | 'dropbox';
export type SourceStatus = 'idle' | 'scanning' | 'unavailable' | 'error';
export type ContainerKind = 'folder_pack' | 'zip_archive' | 'single_file';
export type StorageState = 'local_resident' | 'cloud_placeholder' | 'dropbox_remote' | 'zip_entry';

export interface SourceDto {
  id: number;
  kind: SourceKind;
  name: string;
  rootUri: string;
  rootDisplayPath: string;
  status: SourceStatus;
  lastError: string | null;
  lastScanAt: number | null;
  modelCount: number;
  fileCount: number;
}

export interface FolderTreeNode {
  id: number;
  sourceId: number;
  parentId: number | null;
  relPath: string;
  name: string;
  recursiveModelCount: number;
  recursiveFileCount: number;
  nestedFolderCount: number;
  children: FolderTreeNode[];
}

export interface FolderCardDto {
  id: number;
  sourceId: number;
  relPath: string;
  name: string;
  recursiveModelCount: number;
  recursiveFileCount: number;
  nestedFolderCount: number;
  sampleTitles: string[];
  hasCloudPlaceholder: boolean;
}

export interface ModelCardDto {
  id: number;
  sourceId: number;
  folderId: number | null;
  containerKind: ContainerKind;
  canonicalUri: string;
  displayTitle: string;
  rawName: string;
  relativePath: string;
  primaryExt: string | null;
  hasCloudPlaceholder: boolean;
  fileCount: number;
  totalBytes: number;
  updatedAt: number;
}

export interface FileCardDto {
  id: number;
  modelId: number | null; // Standalone images have no model Inspector.
  sourceId: number;
  folderId: number | null;
  parentArchiveId: number | null;
  parentArchiveName: string | null;
  uri: string;
  relPath: string;
  filename: string;
  ext: string;
  storageState: StorageState;
  sizeBytes: number;
  mtimeUnix: number | null;
}

export interface SearchQuery {
  text: string;
  viewMode: ViewMode;
  sourceFilter: SourceFilter;
  sourceId?: number | null;
  folderId: number | null;
  facets: Record<string, string[]>;
  limit: number;
  offset: number;
}

export interface SearchResultPage {
  totalModels: number;
  totalFiles: number;
  subfolders: FolderCardDto[];
  items: ModelCardDto[] | FileCardDto[];
  facetCounts: Record<string, Array<{ slug: string; labelIt: string; count: number }>>;
  queryTimeMs: number;
}

export interface ModelDetailDto {
  id: number;
  sourceId: number;
  sourceName: string;
  containerKind: ContainerKind;
  canonicalUri: string;
  displayTitle: string;
  rawName: string;
  primaryExt: string | null;
  hasCloudPlaceholder: boolean;
  fileCount: number;
  totalBytes: number;
  updatedAt: number;
  files: FileCardDto[];
}

export interface ScanStatusDto {
  isRunning: boolean;
  activeSourceId: number | null;
  pass: 1 | 2 | null;
  phaseLabel: string | null;
  processed: number;
  total: number;
  currentItem: string | null;
  recentErrors: IndexerErrorEvent[];
}

export type IpcErrorCode =
  | 'SOURCE_NOT_FOUND'
  | 'SOURCE_UNAVAILABLE'
  | 'SOURCE_OVERLAP'
  | 'INVALID_PATH'
  | 'SCAN_ALREADY_RUNNING'
  | 'MODEL_NOT_FOUND'
  | 'DATABASE_ERROR'
  | 'INTERNAL_ERROR';

export interface IpcError {
  code: IpcErrorCode;
  message: string;
  details?: string;
}

export interface IndexerProgressEvent {
  sourceId: number;
  pass: 1 | 2 | 3;
  phaseLabel: string;
  processed: number;
  total: number;
  currentItem: string;
}

export interface IndexerBatchUpdatedEvent {
  sourceId: number;
  updatedFolderIds: number[];
  updatedModelIds: number[];
}

export interface IndexerErrorEvent {
  sourceId: number;
  message: string;
  path?: string;
  recoverable: boolean;
}

export interface IndexerCompletedEvent {
  sourceId: number | null;
  cancelled: boolean;
  durationMs: number;
  modelsIndexed: number;
  filesIndexed: number;
  errorsCount: number;
}

export interface IndexerEvents {
  'indexer://progress': IndexerProgressEvent;
  'indexer://batch_updated': IndexerBatchUpdatedEvent;
  'indexer://error': IndexerErrorEvent;
  'indexer://completed': IndexerCompletedEvent;
}
