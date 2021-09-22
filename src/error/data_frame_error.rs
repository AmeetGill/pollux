use std::fmt;

type Result<T> = std::result::Result<T, DataFrameError>;

#[derive(Debug)]
enum ErrorKind {
    InvalidOpcode,
    UnknownError
}

#[derive(Debug)]
struct DataFrameError {
    message: String,
    error_kind: ErrorKind
}

impl fmt::Display for DataFrameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"Data Frame Error occurred, ErrorKind: {:?}, message: {}",self.error_kind,self.message)
    }
}
