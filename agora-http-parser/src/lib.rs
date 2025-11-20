use std::fmt::Display;

pub enum HTTPVersion {
    HTTP1_1,
    HTTP2,
    HTTP3,
}

pub enum HTTPMethod {
    GET,
}

pub struct Request {
    pub path: String,
    pub method: HTTPMethod,
    pub headers: Headers,
    pub version: HTTPVersion,
}

#[derive(Debug)]
pub enum HTTPParseError {
    UnterminatedMessage,
}

type Headers = String;

impl Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

const CRLF: &[u8; 2] = b"\r\n";

impl Request {
    pub fn parse(buf: &[u8]) -> Result<Self, HTTPParseError> {
        if !buf.ends_with(b"\r\n\r\n") {
            return Err(HTTPParseError::UnterminatedMessage);
        }

        let () = Self::parse_start_line()?;
        let headers = Self::parse_headers()?;

        Ok(Self {
            path: String::new(),
            method: HTTPMethod::GET,
            headers: String::new(),
            version: HTTPVersion::HTTP1_1,
        })
    }

    fn parse_start_line() -> Result<(), HTTPParseError> {
        todo!()
    }

    fn parse_headers() -> Result<Headers, HTTPParseError> {
        todo!()
    }
}
