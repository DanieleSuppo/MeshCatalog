use meshcatalog_core::{contracts::*, error::IpcError};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

fn assert_wire<T: DeserializeOwned + Serialize>(wire: Value) {
    let dto: T = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(dto).unwrap(), wire);
}

#[test]
fn search_contract_preserves_distinct_path_names_and_untagged_items() {
    assert_wire::<SearchQuery>(json!({
        "text": ".stl", "viewMode": "models", "sourceFilter": "local", "folderId": null,
        "facets": {}, "limit": 20, "offset": 0
    }));
    assert_wire::<SearchItemDto>(json!({
        "id": 2, "sourceId": 1, "folderId": null, "containerKind": "zip_archive",
        "canonicalUri": "local://D:/Pack.zip", "displayTitle": "Pack", "rawName": "Pack.zip",
        "relativePath": "Pack.zip", "primaryExt": "zip", "hasCloudPlaceholder": false,
        "fileCount": 2, "totalBytes": 100, "updatedAt": 1000
    }));
    assert_wire::<SearchResultPage>(json!({
        "totalModels": 1, "totalFiles": 2, "subfolders": [],
        "items": [{"id": 4, "modelId": 2, "sourceId": 1, "folderId": null, "parentArchiveId": 3,
                   "parentArchiveName": "Pack.zip", "uri": "zip://local://D:/Pack.zip#part.stl",
                   "relPath": "Pack.zip/part.stl", "filename": "part.stl", "ext": "stl",
                   "storageState": "zip_entry", "sizeBytes": 30, "mtimeUnix": null}],
        "facetCounts": {"format": [{"slug": "stl", "labelIt": "STL", "count": 1}]}, "queryTimeMs": 0.25
    }));
}

#[test]
fn search_requires_the_nullable_folder_id() {
    let missing_folder = json!({
        "text": "", "viewMode": "files", "sourceFilter": "all",
        "facets": {}, "limit": 20, "offset": 0
    });
    assert!(serde_json::from_value::<SearchQuery>(missing_folder).is_err());
}

#[test]
fn status_and_events_use_numeric_passes_and_omit_optional_paths() {
    assert_wire::<ScanStatusDto>(json!({
        "isRunning": true, "activeSourceId": 1, "pass": 2, "phaseLabel": "Hash",
        "processed": 0, "total": 2, "currentItem": null,
        "recentErrors": [{ "sourceId": 1, "message": "Unreadable archive", "recoverable": true }]
    }));
    assert_wire::<IndexerProgressEvent>(json!({
        "sourceId": 1, "pass": 3, "phaseLabel": "Geometry", "processed": 1, "total": 2, "currentItem": "Pack"
    }));
    assert!(serde_json::from_value::<ScanPass>(json!(3)).is_err());
    assert!(serde_json::from_value::<IndexerPass>(json!(0)).is_err());
    assert_wire::<IndexerBatchUpdatedEvent>(
        json!({"sourceId": 1, "updatedFolderIds": [2], "updatedModelIds": [3]}),
    );
    assert_wire::<IndexerCompletedEvent>(json!({
        "sourceId": null, "cancelled": false, "durationMs": 42, "modelsIndexed": 2, "filesIndexed": 3, "errorsCount": 0
    }));
}

#[test]
fn errors_expose_stable_codes_without_leaking_database_errors() {
    let examples = [
        (IpcError::SourceNotFound, "SOURCE_NOT_FOUND"),
        (IpcError::SourceUnavailable, "SOURCE_UNAVAILABLE"),
        (IpcError::SourceOverlap, "SOURCE_OVERLAP"),
        (IpcError::InvalidPath, "INVALID_PATH"),
        (IpcError::ScanAlreadyRunning, "SCAN_ALREADY_RUNNING"),
        (IpcError::ModelNotFound, "MODEL_NOT_FOUND"),
        (
            IpcError::from(rusqlite::Error::InvalidPath("private-dir".into())),
            "DATABASE_ERROR",
        ),
        (IpcError::InternalError, "INTERNAL_ERROR"),
    ];
    for (error, code) in examples {
        let wire = serde_json::to_value(error).unwrap();
        assert_eq!(wire["code"], code);
        assert!(!wire["message"].as_str().unwrap().is_empty());
        assert!(!wire.to_string().contains("private-dir"));
        assert!(wire.get("details").is_none());
    }
}
