pub fn parse_socket_addr(s: &str) -> Result<std::net::SocketAddr, std::net::AddrParseError> {
    if s.starts_with(':') {
        format!("127.0.0.1{s}").parse()
    } else {
        s.parse()
    }
}

pub fn parse_duration_secs(s: &str) -> Result<std::time::Duration, std::num::ParseFloatError> {
    let secs = s.parse()?;
    Ok(std::time::Duration::from_secs_f32(secs))
}
