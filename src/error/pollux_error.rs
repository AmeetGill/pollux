use std::fmt;

pub type Result<T> = std::result::Result<T, PolluxError>;

#[derive(Debug)]
pub enum ErrorKind {
    InvalidConfiguration
}

#[derive(Debug)]
pub struct PolluxError {
    message: String,
    error_kind: ErrorKind
}

impl fmt::Display for PolluxError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"Data Frame Error occurred, ErrorKind: {:?}, message: {}",self.error_kind,self.message)
    }
}
