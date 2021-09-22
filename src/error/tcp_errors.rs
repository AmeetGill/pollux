use std::fmt;

type Result<T> = std::result::Result<T, TCPError>;

#[derive(Debug)]
enum ErrorKind {
    TCPDropped
}

#[derive(Debug)]
struct TCPError {
    message: String,
    error_kind: ErrorKind
}

impl fmt::Display for TCPError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"Data Frame Error occurred, ErrorKind: {:?}, message: {}",self.error_kind,self.message)
    }
}
