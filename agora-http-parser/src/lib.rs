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
    pub fn parse(buf: &'a [u8]) -> Result<Self, HTTPParseError> {
        let (path, method, version, buf) = Self::parse_start_line(buf)?;
        let headers = Self::parse_headers(buf)?;

        Ok(Self {
            path: path.to_string(),
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
    fn parse_headers(buf: &'a [u8]) -> Result<Headers, HTTPParseError> {
        let mut headers = HashMap::new();

        let mut line_start = 0;
        let mut separator = 0;
        for i in 0..buf.len() - 1 {
            if buf[i] == b':' {
                separator = i;
            }

            // From the loop, i is at most buf.len() - 2.
            // Therefore, +2 here makes our upper bound at most buf.len(), so this bound will
            // always be valid
            if &buf[i..i + 2] == CRLF
                && line_start < buf.len()
                && separator < buf.len()
                && line_start < separator
            {
                let Ok(key) = str::from_utf8(&buf[line_start..separator]) else {
                    return Err(HTTPParseError::InvalidHeader);
                };
                let Ok(value) = str::from_utf8(&buf[separator + 1..i]) else {
                    return Err(HTTPParseError::InvalidHeader);
                };

                headers.insert(key.to_string(), value.trim().to_string());
                line_start = i + 2;
            }

            // In this case, we read to the end of the buffer,
            // but we are still expecting more headers
            if line_start >= buf.len() {
                return Err(HTTPParseError::UnterminatedHeader);
            }
        }

        Ok(headers)
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
    #[case(b"GET / HTTP/1.1\r\n\r\n", Ok((HTTPMethod::GET, b"/ HTTP/1.1\r\n\r\n".as_slice())))]
    #[case(b"POST /api HTTP/1.1\r\n\r\n", Ok((HTTPMethod::POST, b"/api HTTP/1.1\r\n\r\n".as_slice())))]
    #[case(b"PUT /api HTTP/1.1\r\n\r\n", Ok((HTTPMethod::PUT, b"/api HTTP/1.1\r\n\r\n".as_slice())))]
    #[case(b"PATCH /api HTTP/1.1\r\n\r\n", Ok((HTTPMethod::PATCH, b"/api HTTP/1.1\r\n\r\n".as_slice())))]
    #[case(b"DELETE /api HTTP/1.1\r\n\r\n", Ok((HTTPMethod::DELETE, b"/api HTTP/1.1\r\n\r\n".as_slice())))]
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
    #[case(b"HTTP/1.1\r\nConnection: close\r\n\r\n", Ok((HTTPVersion::HTTP1_1, b"Connection: close\r\n\r\n".as_slice())))]
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
}
