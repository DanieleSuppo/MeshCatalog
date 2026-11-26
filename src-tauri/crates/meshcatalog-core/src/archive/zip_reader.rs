use std::{
    io::{self, BufReader, Read, Seek},
    path::Path,
};

use zip::{result::ZipResult, ZipArchive};

use crate::scanner::exclusions::{is_excluded, supported_extension};

pub(crate) struct ZipEntry {
    pub path: String,
    pub ext: String,
    pub size: i64,
    pub compressed_size: i64,
    pub offset: i64,
    pub method: u16,
}

pub(crate) fn read_zip_directory(reader: impl Read + Seek) -> ZipResult<Vec<ZipEntry>> {
    let mut archive = ZipArchive::new(BufReader::new(reader))?;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index)?;
        if entry.encrypted() {
            return Err(zip::result::ZipError::UnsupportedArchive(
                "Archivio protetto da password",
            ));
        }
        if !entry.is_file() || entry.enclosed_name().is_none() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let mut components = name.split('/');
        if components
            .clone()
            .any(|part| part.is_empty() || matches!(part, "." | "..") || is_excluded(part.as_ref()))
            || components.next().is_some_and(|part| part.contains(':'))
        {
            continue;
        }
        let Some(ext) = supported_extension(Path::new(&name)) else {
            continue;
        };
        let sqlite_size = |value| {
            i64::try_from(value).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Dimensione ZIP fuori intervallo",
                )
            })
        };
        #[allow(deprecated)] // Persist the ZIP method number; no decompressor is used.
        let method = entry.compression().to_u16();
        entries.push(ZipEntry {
            path: name,
            ext,
            size: sqlite_size(entry.size())?,
            compressed_size: sqlite_size(entry.compressed_size())?,
            offset: sqlite_size(entry.header_start())?,
            method,
        });
    }
    entries.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    entries.dedup_by(|a, b| a.path == b.path);
    Ok(entries)
}
