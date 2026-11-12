use std::path::PathBuf;

/// Erreur commune à toutes les opérations de `fo-core`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path} : {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("fichier introuvable : {0}")]
    NotFound(String),
    #[error("données invalides : {0}")]
    Format(String),
    #[error("données tronquées ({what}, à l'octet {at})")]
    Truncated { what: &'static str, at: usize },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }

    pub fn format(msg: impl Into<String>) -> Self {
        Error::Format(msg.into())
    }
}
