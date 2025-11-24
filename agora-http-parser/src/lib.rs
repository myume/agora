use std::{
    collections::HashMap,
    fmt::{Debug, Display},
};

use http::StatusCode;

#[derive(Debug, PartialEq)]
pub enum HTTPVersion {
    HTTP1_1,
    HTTP2,
    HTTP3,
}

#[derive(Debug, PartialEq)]
pub enum HTTPMethod {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,

    HEAD,
    CONNECT,
    OPTIONS,
    TRACE,
}

#[derive(Debug)]
pub struct Request {
    pub path: String,
    pub method: HTTPMethod,
    pub headers: Headers,
    pub version: HTTPVersion,
}

#[derive(Debug, PartialEq)]
pub enum HTTPParseError {
    UnterminatedHeader,
    InvalidMethod,
    InvalidVersion,
    InvalidHeader,
    InvalidPath,
}

type Headers = HashMap<String, String>;

impl Display for HTTPParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                HTTPParseError::UnterminatedHeader => "Header is not terminated",
                HTTPParseError::InvalidMethod => "Invalid HTTP method",
                HTTPParseError::InvalidVersion => "Invalid HTTP version",
                HTTPParseError::InvalidHeader => "Invalid HTTP headers",
                HTTPParseError::InvalidPath => "Invalid HTTP Path",
            }
        )
    }
}

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

impl Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} {} {}\n {:#?}",
            self.method, self.path, self.version, self.headers
        )
    }
}

const CRLF: &[u8; 2] = b"\r\n";

impl<'a> Request {
    /// Parse the buffer into a [`Request`]
    pub fn parse(buf: &'a [u8]) -> Result<(Self, &'a [u8]), HTTPParseError> {
        let (path, method, version, buf) = Self::parse_start_line(buf)?;
        let (headers, buf) = Self::parse_headers(buf)?;

        Ok((Self {
            path: path.to_string(),
            method,
            headers,
            version,
        }, buf))
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

    /// Parse the path from the buffer and return the remainging bytes
    fn parse_path(buf: &'a [u8]) -> Result<(&'a str, &'a [u8]), HTTPParseError> {
        let Ok(path) = str::from_utf8(Self::parse_until_space(buf)) else {
            return Err(HTTPParseError::InvalidPath);
        };

        // will need a path validator here
        if path.is_empty() || !path.starts_with("/") {
            return Err(HTTPParseError::InvalidPath);
        }

        Ok((path, &buf[path.len() + 1..]))
    }

    /// Parse the http method from the buffer and return the remainging bytes
    fn parse_method(buf: &'a [u8]) -> Result<(HTTPMethod, &'a [u8]), HTTPParseError> {
        let method = Self::parse_until_space(buf);
        Ok((method.try_into()?, &buf[method.len() + 1..]))
    }

    /// Parse the http version from the buffer and return the remainging bytes
    fn parse_version(buf: &'a [u8]) -> Result<(HTTPVersion, &'a [u8]), HTTPParseError> {
        let version = Self::parse_until_crlf(buf);

        // + 2 here to skip over CRLF
        Ok((version.try_into()?, &buf[version.len() + 2..]))
    }

    /// Parse the headers from the buffer
    fn parse_headers(mut buf: &'a [u8]) -> Result<(Headers, &'a [u8]), HTTPParseError> {
        let mut headers = HashMap::new();

        // buf is the start of the current line
        // loop will stop when we either find a crlf at the start of the line indicating the end,
        // or we don't have a crlf terminator at all
        while buf.len() >= 2 && &buf[..2] != CRLF {
            let (key, value, rest) = Self::parse_header(buf)?;

            headers.insert(key.to_string(), value.to_string());
            buf = rest;

            dbg!(str::from_utf8(buf).unwrap(), key, value);
        }

        // loop terminated because we don't have a crlf terminator
        if buf.len() < 2 {
            return Err(HTTPParseError::UnterminatedHeader);
        }

        // otherwise the current line starts with crlf, so we've reached the end of the headers

        Ok((headers, &buf[2..]))
    }

    fn parse_header(buf: &'a [u8]) -> Result<(&'a str, &'a str, &'a [u8]), HTTPParseError> {
        let mut separator_index = None;
        for i in 0..buf.len() - 1 {
            if buf[i] == b':' {
                separator_index = Some(i);
            }

            if &buf[i..i + 2] == CRLF {
                if let Some(separator_index) = separator_index {
                    return Ok((
                        str::from_utf8(&buf[..separator_index])
                            .map_err(|_| HTTPParseError::InvalidHeader)?,
                        str::from_utf8(&buf[separator_index + 1..i])
                            .map_err(|_| HTTPParseError::InvalidHeader)?
                            .trim(),
                        &buf[i + 2..],
                    ));
                } else {
                    return Err(HTTPParseError::UnterminatedHeader);
                }
            }
        }

        Err(HTTPParseError::UnterminatedHeader)
    }

    fn parse_until_space(buf: &[u8]) -> &[u8] {
        for i in 0..buf.len() {
            if buf[i] == b' ' {
                return &buf[..i];
            }
        }

        // if we couldn't find a space, return empty string
        b""
    }

    fn parse_until_crlf(buf: &[u8]) -> &[u8] {
        for i in 0..buf.len() {
            if i < buf.len() - 1 && &buf[i..i + 2] == CRLF {
                return &buf[..i];
            }
        }

        b""
    }

    pub fn insert_header(&mut self, key: &'a str, value: &'a str) {
        self.headers.insert(key.to_string(), value.to_string());
    }

    pub fn into_bytes(&self) -> Vec<u8> {
        let mut request = format!("{:?} {} {}\r\n", self.method, self.path, self.version);
        for (key, value) in &self.headers {
            request.push_str(&format!("{}: {}\r\n", key, value));
        }
        request.push_str("\r\n");

        request.into_bytes()
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
            b"HEAD" => Ok(HTTPMethod::HEAD),
            b"PATCH" => Ok(HTTPMethod::PATCH),
            b"TRACE" => Ok(HTTPMethod::TRACE),
            b"OPTIONS" => Ok(HTTPMethod::OPTIONS),
            b"CONNECT" => Ok(HTTPMethod::CONNECT),
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

pub struct Response<'a> {
    status: StatusCode,
    version: HTTPVersion,
    headers: Headers,
    body: &'a [u8],
}

impl<'a> Response<'a> {
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            version: HTTPVersion::HTTP1_1, // Hardcode to HTTP/1.1
            headers: HashMap::new(),
            body: &[],
        }
    }

    pub fn header(&mut self, key: &str, value: &str) {
        self.headers.insert(key.to_string(), value.to_string());
    }

    pub fn body(&mut self, body: &'a [u8]) {
        self.body = body;
    }

    pub fn into_bytes(&self) -> Vec<u8> {
        let mut response = format!(
            "{} {} {}\r\n",
            self.version,
            self.status.as_u16(),
            self.status.canonical_reason().unwrap_or("Unknown Reason")
        );

        let mut has_content_length = false;
        for (key, value) in &self.headers {
            has_content_length = has_content_length || key.to_lowercase() == "content-length";
            response.push_str(&format!("{}: {}\r\n", key, value));
        }
        if !has_content_length {
            response.push_str(&format!("Content-Length: {}\r\n", self.body.len()));
        }
        response.push_str("\r\n");

        let mut bytes = response.into_bytes();
        bytes.extend_from_slice(self.body);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(b"hello world", b"hello")]
    #[case(b"helloworld", b"")]
    #[case(b"HTTP/1.1\r\n", b"")]
    #[case(b"HTTP/1.1 200 OK", b"HTTP/1.1")]
    fn test_parse_until_space(#[case] input: &[u8], #[case] expected: &[u8]) {
        assert_eq!(expected, Request::parse_until_space(input));
    }

    #[rstest]
    #[case(b"hello world", b"")]
    #[case(b"hello world\r\n", b"hello world")]
    #[case(b"HTTP/1.1\r\n", b"HTTP/1.1")]
    fn test_parse_until_crlf(#[case] input: &[u8], #[case] expected: &[u8]) {
        assert_eq!(expected, Request::parse_until_crlf(input));
    }

    #[rstest]
    #[case(
        b"GET / HTTP/1.1\r\n\r\n",
        Ok((HTTPMethod::GET, b"/ HTTP/1.1\r\n\r\n".as_slice())))
    ]
    #[case(
        b"POST /api HTTP/1.1\r\n\r\n",
        Ok((HTTPMethod::POST, b"/api HTTP/1.1\r\n\r\n".as_slice())))
    ]
    #[case(
        b"PUT /api HTTP/1.1\r\n\r\n",
        Ok((HTTPMethod::PUT, b"/api HTTP/1.1\r\n\r\n".as_slice())))
    ]
    #[case(
        b"PATCH /api HTTP/1.1\r\n\r\n",
        Ok((HTTPMethod::PATCH, b"/api HTTP/1.1\r\n\r\n".as_slice())))
    ]
    #[case(
        b"DELETE /api HTTP/1.1\r\n\r\n",
        Ok((HTTPMethod::DELETE, b"/api HTTP/1.1\r\n\r\n".as_slice())))
    ]
    #[case(b"INVALID / HTTP/1.1\r\n\r\n", Err(HTTPParseError::InvalidMethod))]
    #[case(b"GET\r\n/\r\nHTTP/1.1\r\n", Err(HTTPParseError::InvalidMethod))]
    #[case(b"GET/ HTTP/1.1\r\n\r\n", Err(HTTPParseError::InvalidMethod))]
    #[case(b"/ HTTP/1.1\r\n\r\n", Err(HTTPParseError::InvalidMethod))]
    #[case(b" / HTTP/1.1\r\n\r\n", Err(HTTPParseError::InvalidMethod))]
    fn test_parse_method(
        #[case] input: &[u8],
        #[case] expected: Result<(HTTPMethod, &[u8]), HTTPParseError>,
    ) {
        assert_eq!(expected, Request::parse_method(input));
    }

    #[rstest]
    #[case(b"/ HTTP/1.1\r\n\r\n", Ok(("/", b"HTTP/1.1\r\n\r\n".as_slice())))]
    #[case(b"/api HTTP/1.1\r\n\r\n", Ok(("/api", b"HTTP/1.1\r\n\r\n".as_slice())))]
    #[case(b"/stuff-with-dashes HTTP/1.1\r\n\r\n", Ok(("/stuff-with-dashes", b"HTTP/1.1\r\n\r\n".as_slice())))]
    #[case(b"not-a-path HTTP/1.1\r\n\r\n", Err(HTTPParseError::InvalidPath))]
    #[case(b" HTTP/1.1\r\n\r\n", Err(HTTPParseError::InvalidPath))]
    #[case(b"HTTP/1.1\r\n\r\n", Err(HTTPParseError::InvalidPath))]
    fn test_parse_path(
        #[case] input: &[u8],
        #[case] expected: Result<(&str, &[u8]), HTTPParseError>,
    ) {
        assert_eq!(expected, Request::parse_path(input));
    }

    #[rstest]
    #[case(b"HTTP/1.1\r\n\r\n", Ok((HTTPVersion::HTTP1_1, b"\r\n".as_slice())))]
    #[case(
        b"HTTP/1.1\r\nConnection: close\r\n\r\n", 
        Ok((HTTPVersion::HTTP1_1, b"Connection: close\r\n\r\n".as_slice())))
    ]
    #[case(b"HTTP/2\r\n\r\n", Ok((HTTPVersion::HTTP2, b"\r\n".as_slice())))]
    #[case(b"HTTP/3\r\n\r\n", Ok((HTTPVersion::HTTP3, b"\r\n".as_slice())))]
    #[case(b"HTTP/100\r\n\r\n", Err(HTTPParseError::InvalidVersion))]
    #[case(b"invalid version\r\n", Err(HTTPParseError::InvalidVersion))]
    #[case(b"non-terminated request line", Err(HTTPParseError::InvalidVersion))]
    #[case(b"", Err(HTTPParseError::InvalidVersion))]
    fn test_parse_version(
        #[case] input: &[u8],
        #[case] expected: Result<(HTTPVersion, &[u8]), HTTPParseError>,
    ) {
        assert_eq!(expected, Request::parse_version(input));
    }

    #[rstest]
    #[case(
        b"Host: test\r\nConnection: keep-alive\r\nAccept: text/html\r\n\r\n",
        Ok((HashMap::from([
            ("Host".to_string(), "test".to_string()),
            ("Connection".to_string(), "keep-alive".to_string()),
            ("Accept".to_string(), "text/html".to_string()),
        ]), 
        b"".as_slice()))
    )]
    #[case(
        b"Host:test\r\nConnection:keep-alive\r\nAccept:text/html\r\n\r\n",
        Ok((HashMap::from([
            ("Host".to_string(), "test".to_string()),
            ("Connection".to_string(), "keep-alive".to_string()),
            ("Accept".to_string(), "text/html".to_string()),
        ]), 
        b"".as_slice()))
    )]
    #[case(b"\r\n", Ok((HashMap::from([]), b"".as_slice())))]
    #[case(
        b"Host: test\r\nConnection: keep-alive\r\nAccept: text/html\r\n",
        Err(HTTPParseError::UnterminatedHeader)
    )]
    #[case(b"Host: test", Err(HTTPParseError::UnterminatedHeader))]
    #[case(b"", Err(HTTPParseError::UnterminatedHeader))]
    #[case(b"Connection\r\n", Err(HTTPParseError::UnterminatedHeader))]
    fn test_parse_headers(
        #[case] input: &[u8],
        #[case] expected: Result<(Headers, &[u8]), HTTPParseError>,
    ) {
        assert_eq!(expected, Request::parse_headers(input));
    }
}
