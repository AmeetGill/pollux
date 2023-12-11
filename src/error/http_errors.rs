use std::fmt;
use crate::data_frame::Opcode;

type Result<T> = std::result::Result<T, HTTPError>;

#[derive(Debug)]
enum ErrorKind {
    InvalidOpcode(Opcode),
    UnknownError(String)
}

#[derive(Debug)]
struct HTTPError {
    message: String,
    error_kind: ErrorKind
}

impl fmt::Display for HTTPError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"Data Frame Error occurred, message ErrorKind: {:?}, message: {}",self.error_kind,self.message)
    }
}
