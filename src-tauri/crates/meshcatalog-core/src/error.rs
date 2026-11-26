use serde::{ser::SerializeStruct, Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IpcErrorCode {
    SourceNotFound,
    SourceUnavailable,
    SourceOverlap,
    InvalidPath,
    ScanAlreadyRunning,
    ModelNotFound,
    DatabaseError,
    InternalError,
}

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("Sorgente non trovata.")]
    SourceNotFound,
    #[error("Sorgente non accessibile.")]
    SourceUnavailable,
    #[error("La cartella si sovrappone a una sorgente già registrata.")]
    SourceOverlap,
    #[error("Il percorso non è una cartella valida.")]
    InvalidPath,
    #[error("Una scansione è già in corso per questa sorgente.")]
    ScanAlreadyRunning,
    #[error("Modello non trovato.")]
    ModelNotFound,
    #[error("Operazione sul catalogo non riuscita.")]
    DatabaseError(#[from] rusqlite::Error),
    #[error("Versione dello schema del catalogo non supportata: {0}.")]
    UnsupportedSchema(i64),
    #[error("Impossibile attivare la modalità WAL per il catalogo.")]
    WalUnavailable,
    #[error("Errore interno.")]
    InternalError,
}

impl IpcError {
    pub fn code(&self) -> IpcErrorCode {
        match self {
            Self::SourceNotFound => IpcErrorCode::SourceNotFound,
            Self::SourceUnavailable => IpcErrorCode::SourceUnavailable,
            Self::SourceOverlap => IpcErrorCode::SourceOverlap,
            Self::InvalidPath => IpcErrorCode::InvalidPath,
            Self::ScanAlreadyRunning => IpcErrorCode::ScanAlreadyRunning,
            Self::ModelNotFound => IpcErrorCode::ModelNotFound,
            Self::DatabaseError(_) | Self::UnsupportedSchema(_) | Self::WalUnavailable => {
                IpcErrorCode::DatabaseError
            }
            Self::InternalError => IpcErrorCode::InternalError,
        }
    }
}

impl Serialize for IpcError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("IpcError", 2)?;
        state.serialize_field("code", &self.code())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}
