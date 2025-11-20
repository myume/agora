use std::{
    fmt::{Debug, Display},
    str::Utf8Error,
};

#[derive(Debug)]
pub enum HTTPVersion {
    HTTP1_1,
    HTTP2,
    HTTP3,
}

#[derive(Debug)]
pub enum HTTPMethod {
    GET,
    POST,
    PUT,
    DELETE,
}

#[derive(Debug)]
pub struct Request<'a> {
    pub path: &'a str,
    pub method: HTTPMethod,
    pub headers: Headers,
    pub version: HTTPVersion,
}

#[derive(Debug)]
pub enum HTTPParseError {
    UnterminatedMessage,
    InvalidMethod,
    InvalidVersion,
    NonUTF8Path(Utf8Error),
}

type Headers = String;

impl Display for HTTPVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "HTTP/{}",
            match self {
                HTTPVersion::HTTP1_1 => "1.1",
                HTTPVersion::HTTP2 => "2",
                HTTPVersion::HTTP3 => "3",
            }
        )
    }
}

impl<'a> Display for Request<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} {} {}", self.method, self.path, self.version)
    }
}

const CRLF: &[u8; 2] = b"\r\n";

impl<'a> Request<'a> {
    pub fn parse(buf: &'a [u8]) -> Result<Self, HTTPParseError> {
        let (path, method, version, buf) = Self::parse_start_line(buf)?;
        let headers = Self::parse_headers(buf)?;

        Ok(Self {
            path,
            method,
            headers,
            version,
        })
    }

    /// Parse the buffer for the HTTP start line from the start to the first CRLF
    /// Returns the path, method, and version, and remainging bytes in this exact order
    fn parse_start_line(
        buf: &'a [u8],
    ) -> Result<(&'a str, HTTPMethod, HTTPVersion, &'a [u8]), HTTPParseError> {
        let (method, buf) = Self::parse_method(buf)?;
        let (path, buf) = Self::parse_path(buf)?;
        let (version, buf) = Self::parse_version(buf)?;

        Ok((path, method, version, buf))
    }

    fn parse_path(buf: &'a [u8]) -> Result<(&'a str, &'a [u8]), HTTPParseError> {
        let path = Self::parse_until_space_or_crlf(buf);
        Ok((
            str::from_utf8(path).map_err(HTTPParseError::NonUTF8Path)?,
            &buf[path.len() + 1..],
        ))
    }

    fn parse_method(buf: &'a [u8]) -> Result<(HTTPMethod, &'a [u8]), HTTPParseError> {
        let method = Self::parse_until_space_or_crlf(buf);
        Ok((method.try_into()?, &buf[method.len() + 1..]))
    }

    fn parse_version(buf: &'a [u8]) -> Result<(HTTPVersion, &'a [u8]), HTTPParseError> {
        let version = Self::parse_until_space_or_crlf(buf);
        Ok((version.try_into()?, &buf[version.len() + 1..]))
    }

    fn parse_headers(buf: &[u8]) -> Result<Headers, HTTPParseError> {
        Ok(String::new())
    }

    fn parse_until_space_or_crlf(buf: &[u8]) -> &[u8] {
        for i in 0..buf.len() {
            if buf[i] == b' ' || (i < buf.len() - 1 && &buf[i..i + 2] == CRLF) {
                return &buf[..i];
            }
        }

        buf
    }
}

impl TryFrom<&[u8]> for HTTPMethod {
    type Error = HTTPParseError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value {
            b"GET" => Ok(HTTPMethod::GET),
            b"PUT" => Ok(HTTPMethod::PUT),
            b"POST" => Ok(HTTPMethod::POST),
            b"DELETE" => Ok(HTTPMethod::DELETE),
            _ => Err(HTTPParseError::InvalidMethod),
        }
    }
}

impl TryFrom<&[u8]> for HTTPVersion {
    type Error = HTTPParseError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value {
            b"HTTP/1.1" => Ok(HTTPVersion::HTTP1_1),
            b"HTTP/2" => Ok(HTTPVersion::HTTP2),
            b"HTTP/3" => Ok(HTTPVersion::HTTP3),
            _ => Err(HTTPParseError::InvalidVersion),
        }
    }
}
