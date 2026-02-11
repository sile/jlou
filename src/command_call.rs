use std::io::BufRead;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

const MAX_UDP_PACKET: usize = 65507;

pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
    if !noargs::cmd("call")
        .doc("Read JSON-RPC requests from standard input and execute the RPC calls")
        .take(args)
        .is_present()
    {
        return Ok(false);
    }

    let server_addr: SocketAddr = noargs::arg("<SERVER>")
        .doc("JSON-RPC server address or hostname")
        .example("127.0.0.1:8080")
        .take(args)
        .then(|a| crate::utils::parse_socket_addr(a.value()))?;
    let pretty: bool = noargs::flag("pretty")
        .short('p')
        .doc("Pretty-print JSON responses to stdout")
        .take(args)
        .is_present();
    let send_buf_size: std::num::NonZeroUsize = noargs::opt("send-buf-size")
        .short('b')
        .ty("BYTES")
        .doc("Max UDP payload per outgoing packet; requests are joined with '\\n' up to this size")
        .default("1200")
        .take(args)
        .then(|o| o.value().parse())?;
    let timeout: Duration = noargs::opt("timeout")
        .ty("SECONDS")
        .doc("Read timeout for waiting responses")
        .default("5")
        .take(args)
        .then(|o| crate::utils::parse_duration_secs(o.value()))?;

    if args.metadata().help_mode {
        return Ok(true);
    }

    run(server_addr, pretty, send_buf_size.get(), timeout)?;
    Ok(true)
}

fn run(
    server_addr: SocketAddr,
    pretty: bool,
    send_buf_size: usize,
    timeout: Duration,
) -> crate::Result<()> {
    let socket = connect_to_server_udp(server_addr)?;
    socket.set_read_timeout(Some(timeout))?;

    let stdin = std::io::stdin();
    let input_reader = std::io::BufReader::new(stdin.lock());

    let mut send_buf: Vec<u8> = Vec::with_capacity(send_buf_size);
    let mut pending_responses = 0usize;

    for line in input_reader.lines() {
        let line = line?;
        let json = nojson::RawJson::parse(&line)?;
        let has_id = crate::utils::validate_json_rpc_request(json.value())?.is_some();
        let request_len = line.as_bytes().len();

        if request_len > send_buf_size {
            return Err(crate::Error::new("request size exceeds send-buf-size"));
        }

        let extra = if send_buf.is_empty() { 0 } else { 1 };
        if send_buf.len() + extra + request_len > send_buf_size {
            flush_send_buf(&socket, &mut send_buf)?;
        }

        if !send_buf.is_empty() {
            send_buf.push(b'\n');
        }
        send_buf.extend_from_slice(line.as_bytes());

        if has_id {
            pending_responses += 1;
        }
    }

    if !send_buf.is_empty() {
        flush_send_buf(&socket, &mut send_buf)?;
    }

    if pending_responses > 0 {
        receive_responses(&socket, pending_responses, pretty)?;
    }

    Ok(())
}

fn connect_to_server_udp(server_addr: SocketAddr) -> crate::Result<UdpSocket> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect(server_addr)?;
    Ok(socket)
}

fn flush_send_buf(socket: &UdpSocket, send_buf: &mut Vec<u8>) -> crate::Result<()> {
    let size = socket.send(send_buf)?;
    if size != send_buf.len() {
        return Err(crate::Error::new("failed to send complete request packet"));
    }
    send_buf.clear();
    Ok(())
}

fn receive_responses(socket: &UdpSocket, expected: usize, pretty: bool) -> crate::Result<()> {
    let mut recv_buf = vec![0u8; MAX_UDP_PACKET];
    let mut received = 0usize;
    while received < expected {
        let bytes_read = match socket.recv(&mut recv_buf) {
            Ok(size) => size,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                return Err(crate::Error::new(format!(
                    "timed out waiting for responses (received {received} of {expected})"
                )));
            }
            Err(e) => return Err(e.into()),
        };

        let text = std::str::from_utf8(&recv_buf[..bytes_read])?;
        for line in text.lines() {
            if pretty {
                let json = nojson::RawJson::parse(line)?;
                let pretty_json = nojson::json(|f| {
                    f.set_indent_size(2);
                    f.set_spacing(true);
                    f.value(json.value())
                });
                println!("{pretty_json}");
            } else {
                println!("{line}");
            }
            received += 1;
        }
    }
    Ok(())
}
