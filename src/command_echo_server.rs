const MAX_UDP_PACKET: usize = 65535;

pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
    if !noargs::cmd("echo-server")
        .doc(concat!(
            "Run a JSON-RPC echo server\n",
            "\n",
            "This server will respond to every request with a response containing\n",
            "the same request object as the result value."
        ))
        .take(args)
        .is_present()
    {
        return Ok(false);
    }

    let bind_addr = noargs::arg("<ADDR>")
        .doc("UDP bind address (FORMAT: `[IP_ADDR]:PORT`)")
        .example(":9000")
        .take(args)
        .then(|a| crate::utils::parse_socket_addr(a.value()))?;

    if args.metadata().help_mode {
        return Ok(true);
    }

    run_server_udp(bind_addr)?;
    Ok(true)
}

fn reply_err<M>(socket: &std::net::UdpSocket, addr: std::net::SocketAddr, code: i32, message: M)
where
    M: std::fmt::Display,
{
    let response = nojson::object(|f| {
        f.member("jsonrpc", "2.0")?;
        f.member("id", ())?; // null
        f.member(
            "error",
            nojson::object(|f| {
                f.member("code", code)?;
                f.member("message", message.to_string())
            }),
        )
    });
    let _ = socket.send_to(response.to_string().as_bytes(), addr);
}

fn run_server_udp(bind_addr: std::net::SocketAddr) -> crate::Result<()> {
    let socket = std::net::UdpSocket::bind(bind_addr)?;
    let mut recv_buf = [0u8; MAX_UDP_PACKET / 2];
    let mut send_buf = [0u8; MAX_UDP_PACKET];
    loop {
        let (size, peer_addr) = socket.recv_from(&mut recv_buf)?;
        if size == 0 {
            continue;
        }

        let Ok(text) = std::str::from_utf8(&recv_buf[..size])
            .inspect_err(|e| reply_err(&socket, peer_addr, -32700, e))
        else {
            continue;
        };

        let mut send_buf_offset = 0;
        for line in text.lines() {
            let Ok(json) = nojson::RawJson::parse(line)
                .inspect_err(|e| reply_err(&socket, peer_addr, -32700, e))
            else {
                continue;
            };

            let Ok(Some(id)) = parse_request(json.value())
                .inspect_err(|e| reply_err(&socket, peer_addr, -32600, e))
            else {
                continue;
            };

            let response = nojson::object(|f| {
                f.member("jsonrpc", "2.0")?;
                f.member("id", id)?;
                f.member("result", &json)
            })
            .to_string();
            let size = response.as_bytes().len();
            send_buf[send_buf_offset..][..size].copy_from_slice(response.as_bytes());
            send_buf_offset += size;
        }

        let _ = socket.send_to(&send_buf[..send_buf_offset], peer_addr);
    }
}

fn parse_request<'text, 'raw>(
    value: nojson::RawJsonValue<'text, 'raw>,
) -> Result<Option<nojson::RawJsonValue<'text, 'raw>>, nojson::JsonParseError> {
    if value.kind() == nojson::JsonValueKind::Array {
        return Err(value.invalid("batch requests are not supported"));
    }

    let mut has_jsonrpc = false;
    let mut has_method = false;
    let mut id = None;
    for (name, value) in value.to_object()? {
        match name.as_string_str()? {
            "jsonrpc" => {
                if value.as_string_str()? != "2.0" {
                    return Err(value.invalid("jsonrpc version must be '2.0'"));
                }
                has_jsonrpc = true;
            }
            "id" => {
                if !matches!(
                    value.kind(),
                    nojson::JsonValueKind::Integer | nojson::JsonValueKind::String
                ) {
                    return Err(value.invalid("id must be an integer or string"));
                }
                id = Some(value);
            }
            "method" => {
                if value.kind() != nojson::JsonValueKind::String {
                    return Err(value.invalid("method must be a string"));
                }
                has_method = true;
            }
            "params" => {
                if !matches!(
                    value.kind(),
                    nojson::JsonValueKind::Object | nojson::JsonValueKind::Array
                ) {
                    return Err(value.invalid("params must be an object or array"));
                }
            }
            _ => {
                // Ignore unknown members
            }
        }
    }

    if !has_jsonrpc {
        return Err(value.invalid("jsonrpc field is required"));
    }
    if !has_method {
        return Err(value.invalid("method field is required"));
    }

    Ok(id)
}
