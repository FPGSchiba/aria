use std::fmt::Display;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    JWT(#[from] jsonwebtoken::errors::Error),
    IO(#[from] std::io::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::JWT(e) => e.fmt(f),
            Error::IO(e) => e.fmt(f),
        }
    }
}
