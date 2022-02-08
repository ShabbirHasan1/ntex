//! Http related errors
use std::{fmt, io, io::Write, str::Utf8Error, string::FromUtf8Error};

use http::{header, uri::InvalidUri, StatusCode};

// re-export for convinience
pub use crate::channel::Canceled;
pub use http::Error as HttpError;

use crate::http::body::Body;
use crate::http::response::Response;
use crate::util::{BytesMut, Either};

/// Error that can be converted to `Response`
pub trait ResponseError: fmt::Display + fmt::Debug {
    /// Create response for error
    ///
    /// Internal server error is generated by default.
    fn error_response(&self) -> Response {
        let mut resp = Response::new(StatusCode::INTERNAL_SERVER_ERROR);
        let mut buf = BytesMut::new();
        let _ = write!(crate::http::helpers::Writer(&mut buf), "{}", self);
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        resp.set_body(Body::from(buf))
    }
}

impl ResponseError for std::convert::Infallible {}

impl<'a, T: ResponseError> ResponseError for &'a T {
    fn error_response(&self) -> Response {
        (*self).error_response()
    }
}

impl<T: ResponseError> From<T> for Response {
    fn from(err: T) -> Response {
        let resp = err.error_response();
        if resp.head().status == StatusCode::INTERNAL_SERVER_ERROR {
            error!("Internal Server Error: {:?}", err);
        } else {
            debug!("Error in response: {:?}", err);
        }
        resp
    }
}

/// Return `InternalServerError` for `HttpError`,
/// Response generation can return `HttpError`, so it is internal error
impl ResponseError for HttpError {}

/// Return `InternalServerError` for `io::Error`
impl ResponseError for io::Error {}

/// `InternalServerError` for `JsonError`
impl ResponseError for serde_json::error::Error {}

/// A set of errors that can occur during parsing HTTP streams
#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    /// An invalid `Method`, such as `GE.T`.
    #[error("Invalid Method specified")]
    Method,
    /// An invalid `Uri`, such as `exam ple.domain`.
    #[error("Uri error: {0}")]
    Uri(#[from] InvalidUri),
    /// An invalid `HttpVersion`, such as `HTP/1.1`
    #[error("Invalid HTTP version specified")]
    Version,
    /// An invalid `Header`.
    #[error("Invalid Header provided")]
    Header,
    /// A message head is too large to be reasonable.
    #[error("Message head is too large")]
    TooLarge,
    /// A message reached EOF, but is not complete.
    #[error("Message is incomplete")]
    Incomplete,
    /// An invalid `Status`, such as `1337 ELITE`.
    #[error("Invalid Status provided")]
    Status,
    /// A timeout occurred waiting for an IO event.
    #[allow(dead_code)]
    #[error("Timeout during parse")]
    Timeout,
    /// An `InvalidInput` occurred while trying to parse incoming stream.
    #[error("`InvalidInput` occurred while trying to parse incoming stream: {0}")]
    InvalidInput(&'static str),
    /// Parsing a field as string failed
    #[error("UTF8 error: {0}")]
    Utf8(#[from] Utf8Error),
}

impl From<FromUtf8Error> for ParseError {
    fn from(err: FromUtf8Error) -> ParseError {
        ParseError::Utf8(err.utf8_error())
    }
}

impl From<httparse::Error> for ParseError {
    fn from(err: httparse::Error) -> ParseError {
        match err {
            httparse::Error::HeaderName
            | httparse::Error::HeaderValue
            | httparse::Error::NewLine
            | httparse::Error::Token => ParseError::Header,
            httparse::Error::Status => ParseError::Status,
            httparse::Error::TooManyHeaders => ParseError::TooLarge,
            httparse::Error::Version => ParseError::Version,
        }
    }
}

#[derive(thiserror::Error, Debug)]
/// A set of errors that can occur during payload parsing
pub enum PayloadError {
    /// A payload reached EOF, but is not complete.
    #[error("A payload reached EOF, but is not complete. With error: {0:?}")]
    Incomplete(Option<io::Error>),
    /// Content encoding stream corruption
    #[error("Cannot decode content-encoding.")]
    EncodingCorrupted,
    /// A payload reached size limit.
    #[error("A payload reached size limit.")]
    Overflow,
    /// A payload length is unknown.
    #[error("A payload length is unknown.")]
    UnknownLength,
    /// Http2 payload error
    #[error("{0}")]
    Http2Payload(#[from] h2::Error),
    /// Parse error
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),
    /// Io error
    #[error("{0}")]
    Io(#[from] io::Error),
}

impl From<Either<PayloadError, io::Error>> for PayloadError {
    fn from(err: Either<PayloadError, io::Error>) -> Self {
        match err {
            Either::Left(err) => err,
            Either::Right(err) => PayloadError::Io(err),
        }
    }
}

#[derive(thiserror::Error, Debug)]
/// A set of errors that can occur during dispatching http requests
pub enum DispatchError {
    /// Service error
    #[error("Service error")]
    Service(Box<dyn ResponseError>),

    /// Upgrade service error
    #[error("Upgrade service error: {0}")]
    Upgrade(Box<dyn std::error::Error>),

    /// Peer is disconnected, error indicates that peer is disconnected because of it
    #[error("Disconnected: {0:?}")]
    PeerGone(Option<io::Error>),

    /// Http request parse error.
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),

    /// Http response encoding error.
    #[error("Encode error: {0}")]
    Encode(io::Error),

    /// Http/2 error
    #[error("{0}")]
    H2(#[from] h2::Error),

    /// The first request did not complete within the specified timeout.
    #[error("The first request did not complete within the specified timeout")]
    SlowRequestTimeout,

    /// Disconnect timeout. Makes sense for ssl streams.
    #[error("Connection shutdown timeout")]
    DisconnectTimeout,

    /// Payload is not consumed
    #[error("Task is completed but request's payload is not consumed")]
    PayloadIsNotConsumed,

    /// Malformed request
    #[error("Malformed request")]
    MalformedRequest,

    /// Response body processing error
    #[error("Response body processing error: {0}")]
    ResponsePayload(Box<dyn std::error::Error>),

    /// Internal error
    #[error("Internal error")]
    InternalError,

    /// Unknown error
    #[error("Unknown error")]
    Unknown,
}

impl From<io::Error> for DispatchError {
    fn from(err: io::Error) -> Self {
        DispatchError::PeerGone(Some(err))
    }
}

/// A set of error that can occure during parsing content type
#[derive(thiserror::Error, PartialEq, Debug)]
pub enum ContentTypeError {
    /// Cannot parse content type
    #[error("Cannot parse content type")]
    ParseError,
    /// Unknown content encoding
    #[error("Unknown content encoding")]
    UnknownEncoding,
    /// Unexpected Content-Type
    #[error("Unexpected Content-Type")]
    Unexpected,
    /// Content-Type is expected
    #[error("Content-Type is expected")]
    Expected,
}

/// Blocking operation execution error
#[derive(thiserror::Error, Debug)]
pub enum BlockingError<E: fmt::Debug> {
    #[error("{0:?}")]
    Error(E),
    #[error("Thread pool is gone")]
    Canceled,
}

impl From<crate::rt::JoinError> for PayloadError {
    fn from(_: crate::rt::JoinError) -> Self {
        PayloadError::Io(io::Error::new(
            io::ErrorKind::Other,
            "Operation is canceled",
        ))
    }
}

impl From<BlockingError<io::Error>> for PayloadError {
    fn from(err: BlockingError<io::Error>) -> Self {
        match err {
            BlockingError::Error(e) => PayloadError::Io(e),
            BlockingError::Canceled => PayloadError::Io(io::Error::new(
                io::ErrorKind::Other,
                "Operation is canceled",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{Error as HttpError, StatusCode};
    use std::io;

    #[test]
    fn test_into_response() {
        let err: HttpError = StatusCode::from_u16(10000).err().unwrap().into();
        let resp: Response = err.error_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_error_http_response() {
        let orig = io::Error::new(io::ErrorKind::Other, "other");
        let resp: Response = orig.into();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_payload_error() {
        let err: PayloadError = io::Error::new(io::ErrorKind::Other, "ParseError").into();
        assert!(format!("{}", err).contains("ParseError"));

        let err: PayloadError = BlockingError::Canceled.into();
        assert!(format!("{}", err).contains("Operation is canceled"));

        let err: PayloadError =
            BlockingError::Error(io::Error::new(io::ErrorKind::Other, "ParseError")).into();
        assert!(format!("{}", err).contains("ParseError"));

        let err = PayloadError::Incomplete(None);
        assert_eq!(
            format!("{}", err),
            "A payload reached EOF, but is not complete. With error: None"
        );
    }

    macro_rules! from {
        ($from:expr => $error:pat) => {
            match ParseError::from($from) {
                e @ $error => {
                    assert!(format!("{}", e).len() >= 5);
                }
                e => unreachable!("{:?}", e),
            }
        };
    }

    #[test]
    fn test_from() {
        from!(httparse::Error::HeaderName => ParseError::Header);
        from!(httparse::Error::HeaderName => ParseError::Header);
        from!(httparse::Error::HeaderValue => ParseError::Header);
        from!(httparse::Error::NewLine => ParseError::Header);
        from!(httparse::Error::Status => ParseError::Status);
        from!(httparse::Error::Token => ParseError::Header);
        from!(httparse::Error::TooManyHeaders => ParseError::TooLarge);
        from!(httparse::Error::Version => ParseError::Version);
    }
}
