use agora_http_parser::{Request, Response};
use agora_proxy::server::{ProxyEntry, Server, ServerConfig};
use regex::Regex;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpSocket},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_reverse_proxy_transfer() {
    let response =
        b"HTTP/1.1 200 OK\r\nconnection: close\r\ncontent-length: 12\r\n\r\nTest Success";

    let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = server.local_addr().unwrap();
    let client_socket = TcpSocket::new_v4().unwrap();
    client_socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let client_addr = client_socket.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let (mut stream, _) = server.accept().await.unwrap();
        let mut received = [0; 1024];
        let bytes_read = stream.read(&mut received).await.unwrap();
        let expected = &format!(
            "GET / HTTP/1.1\r\nx-forwarded-for: {}\r\n\r\nHello World",
            client_addr
        )
        .into_bytes();
        let expected_request = Request::parse(expected);

        let actual_request = Request::parse(&received[..bytes_read]);
        assert_eq!(
            actual_request, expected_request,
            "request does not match expected"
        );

        stream.write_all(response).await.unwrap();

        stream.shutdown().await.unwrap();
    });

    let proxy_addr = "127.0.0.1:8080";
    let proxy = tokio::spawn(async move {
        let mut config = ServerConfig::default();
        config.reverse_proxy_mapping.push((
            Regex::new(".*").unwrap(),
            ProxyEntry {
                addr: server_addr.to_string(),
                strip_prefix: false,
            },
        ));
        let server = Server::new(config);

        server.listen(proxy_addr).await.unwrap();
    });

    let client = tokio::spawn(async move {
        let request = b"GET / HTTP/1.1\r\n\r\nHello World";

        let mut stream = client_socket
            .connect(proxy_addr.parse().unwrap())
            .await
            .unwrap();
        stream.write_all(request).await.unwrap();

        let mut received = [0; 1024];
        let bytes_read = stream.read(&mut received).await.unwrap();

        assert_eq!(
            Response::parse(response),
            Response::parse(&received[..bytes_read]),
            "response does not match expected"
        );

        stream.shutdown().await.unwrap();
    });

    server_handle.await.unwrap();
    client.await.unwrap();
    proxy.abort();
}
