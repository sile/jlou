pub fn parse_socket_addr(s: &str) -> Result<std::net::SocketAddr, std::net::AddrParseError> {
    if s.starts_with(':') {
        format!("127.0.0.1{s}").parse()
    } else {
        s.parse()
    }
}
