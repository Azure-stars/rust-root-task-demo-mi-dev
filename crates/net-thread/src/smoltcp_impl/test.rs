use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use sel4::debug_println;

use super::TcpSocket;

#[allow(unused)]
pub(crate) fn test_client() {
    const REQUEST: &str = "\
    GET / HTTP/1.1\r\n\
    Host: ident.me\r\n\
    Accept: */*\r\n\
    \r\n";

    let tcp_socket = TcpSocket::new();
    tcp_socket
        .connect(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(49, 12, 234, 183)),
            80,
        ))
        .unwrap();

    let request_buf = REQUEST.as_bytes();

    tcp_socket.send(request_buf).unwrap();
    let mut response_buf = [0; 256];

    let cnt = tcp_socket.recv(&mut response_buf).unwrap();
    let response = core::str::from_utf8(&response_buf[..cnt]).unwrap();
    debug_println!("response: {:?}", response);
}

#[allow(unused)]
pub(crate) fn run_server() {
    fn http_server(stream: TcpSocket) {
        const CONTENT: &str = r#"<html>
    <head>
      <title>Hello, ArceOS</title>
    </head>
    <body>
      <center>
        <h1>Hello, <a href="https://github.com/rcore-os/arceos">ArceOS</a></h1>
      </center>
      <hr>
      <center>
        <i>Powered by <a href="https://github.com/rcore-os/arceos/tree/main/apps/net/httpserver">ArceOS example HTTP server</a> v0.1.0</i>
      </center>
    </body>
    </html>
    "#;

        macro_rules! header {
            () => {
                "\
    HTTP/1.1 200 OK\r\n\
    Content-Type: text/html\r\n\
    Content-Length: {}\r\n\
    Connection: close\r\n\
    \r\n\
    {}"
            };
        }

        let mut requeset = [0; 256];

        let cnt = stream.recv(&mut requeset).unwrap();
        debug_println!("[Net thread] Request size: {} buf: {:?}", cnt, requeset);

        let response_buf = format!(header!(), CONTENT.len(), CONTENT);
        stream.send(response_buf.as_bytes()).unwrap();
        debug_println!(
            "[Net thread] Send size: {} buf: {:?}",
            response_buf.len(),
            response_buf
        );
    }
    let tcp_socket = TcpSocket::new();
    tcp_socket
        .bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 6379))
        .unwrap();
    debug_println!("[Net thread] Finish binding!");
    tcp_socket.listen().unwrap();
    debug_println!("[Net thread] Start listening!");
    loop {
        match tcp_socket.accept() {
            Ok(socket) => {
                http_server(socket);
            }
            Err(_err) => {}
        }
    }
}
