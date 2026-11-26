use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    Models,
    Files,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFilter {
    All,
    Local,
    Dropbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    LocalDir,
    Dropbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Idle,
    Scanning,
    Unavailable,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerKind {
    FolderPack,
    ZipArchive,
    SingleFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageState {
    LocalResident,
    CloudPlaceholder,
    DropboxRemote,
    ZipEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDto {
    pub id: i64,
    pub kind: SourceKind,
    pub name: String,
    pub root_uri: String,
    pub root_display_path: String,
    pub status: SourceStatus,
    pub last_error: Option<String>,
    pub last_scan_at: Option<i64>,
    pub model_count: i64,
    pub file_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderTreeNode {
    pub id: i64,
    pub source_id: i64,
    pub parent_id: Option<i64>,
    pub rel_path: String,
    pub name: String,
    pub recursive_model_count: i64,
    pub recursive_file_count: i64,
    pub nested_folder_count: i64,
    pub children: Vec<FolderTreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderCardDto {
    pub id: i64,
    pub source_id: i64,
    pub rel_path: String,
    pub name: String,
    pub recursive_model_count: i64,
    pub recursive_file_count: i64,
    pub nested_folder_count: i64,
    pub sample_titles: Vec<String>,
    pub has_cloud_placeholder: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCardDto {
    pub id: i64,
    pub source_id: i64,
    pub folder_id: Option<i64>,
    pub container_kind: ContainerKind,
    pub canonical_uri: String,
    pub display_title: String,
    pub raw_name: String,
    pub relative_path: String,
    pub primary_ext: Option<String>,
    pub has_cloud_placeholder: bool,
    pub file_count: i64,
    pub total_bytes: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCardDto {
    pub id: i64,
    pub model_id: Option<i64>,
    pub source_id: i64,
    pub folder_id: Option<i64>,
    pub parent_archive_id: Option<i64>,
    pub parent_archive_name: Option<String>,
    pub uri: String,
    pub rel_path: String,
    pub filename: String,
    pub ext: String,
    pub storage_state: StorageState,
    pub size_bytes: i64,
    pub mtime_unix: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SearchItemDto {
    Model(ModelCardDto),
    File(FileCardDto),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub text: String,
    pub view_mode: ViewMode,
    pub source_filter: SourceFilter,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<i64>,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub folder_id: Option<i64>,
    pub facets: BTreeMap<String, Vec<String>>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultPage {
    pub total_models: i64,
    pub total_files: i64,
    pub subfolders: Vec<FolderCardDto>,
    pub items: Vec<SearchItemDto>,
    pub facet_counts: BTreeMap<String, Vec<FacetCount>>,
    pub query_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacetCount {
    pub slug: String,
    pub label_it: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDetailDto {
    pub id: i64,
    pub source_id: i64,
    pub source_name: String,
    pub container_kind: ContainerKind,
    pub canonical_uri: String,
    pub display_title: String,
    pub raw_name: String,
    pub primary_ext: Option<String>,
    pub has_cloud_placeholder: bool,
    pub file_count: i64,
    pub total_bytes: i64,
    pub updated_at: i64,
    pub files: Vec<FileCardDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatusDto {
    pub is_running: bool,
    pub active_source_id: Option<i64>,
    pub pass: Option<ScanPass>,
    pub phase_label: Option<String>,
    pub processed: u64,
    pub total: u64,
    pub current_item: Option<String>,
    pub recent_errors: Vec<IndexerErrorEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum ScanPass {
    Inventory = 1,
    Metadata = 2,
}

impl TryFrom<u8> for ScanPass {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Inventory),
            2 => Ok(Self::Metadata),
            _ => Err("expected scan pass 1 or 2"),
        }
    }
}

impl From<ScanPass> for u8 {
    fn from(pass: ScanPass) -> Self {
        pass as Self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum IndexerPass {
    Inventory = 1,
    Metadata = 2,
    Geometry = 3,
}

impl TryFrom<u8> for IndexerPass {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Inventory),
            2 => Ok(Self::Metadata),
            3 => Ok(Self::Geometry),
            _ => Err("expected indexer pass 1, 2 or 3"),
        }
    }
}

impl From<IndexerPass> for u8 {
    fn from(pass: IndexerPass) -> Self {
        pass as Self
    }
}

pub const INDEXER_PROGRESS: &str = "indexer://progress";
pub const INDEXER_BATCH_UPDATED: &str = "indexer://batch_updated";
pub const INDEXER_ERROR: &str = "indexer://error";
pub const INDEXER_COMPLETED: &str = "indexer://completed";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerProgressEvent {
    pub source_id: i64,
    pub pass: IndexerPass,
    pub phase_label: String,
    pub processed: u64,
    pub total: u64,
    pub current_item: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerBatchUpdatedEvent {
    pub source_id: i64,
    pub updated_folder_ids: Vec<i64>,
    pub updated_model_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerErrorEvent {
    pub source_id: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub recoverable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerCompletedEvent {
    pub source_id: Option<i64>,
    pub cancelled: bool,
    pub duration_ms: u64,
    pub models_indexed: u64,
    pub files_indexed: u64,
    pub errors_count: u64,
}
