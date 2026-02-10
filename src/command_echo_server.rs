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

fn run_server_udp(bind_addr: std::net::SocketAddr) -> crate::Result<()> {
    let socket = std::net::UdpSocket::bind(bind_addr)?;
    let mut buf = vec![0u8; MAX_UDP_PACKET];
    loop {
        let (bytes_read, peer_addr) = socket.recv_from(&mut buf)?;
        if bytes_read == 0 {
            continue;
        }

        let response = match String::from_utf8(buf[..bytes_read].to_vec()) {
            Ok(text) => match nojson::RawJson::parse(&text) {
                Ok(json) => {
                    let json_value = json.value();
                    match parse_request(json_value) {
                        Ok(Some(request_id)) => {
                            let response = nojson::object(|f| {
                                f.member("jsonrpc", "2.0")?;
                                f.member("id", request_id)?;
                                f.member("result", json_value)
                            });
                            Some(response.to_string())
                        }
                        Ok(None) => None,
                        Err(e) => Some(build_error_response(e.to_string())),
                    }
                }
                Err(e) => Some(build_error_response(e.to_string())),
            },
            Err(e) => Some(build_error_response(e.to_string())),
        };

        if let Some(response) = response {
            let _ = socket.send_to(response.as_bytes(), peer_addr);
        }
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

fn build_error_response(message: String) -> String {
    let response = nojson::object(|f| {
        f.member("jsonrpc", "2.0")?;
        f.member(
            "error",
            nojson::object(|f| {
                // NOTE: For simplicity, we return a fixed error code (-32600) without an id field.
                // In a production implementation, this should handle errors more granularly:
                // - Parse errors should return -32700 without an id
                // - Invalid requests should return -32600 with the id if present
                f.member("code", -32600)?; // invalid-request code
                f.member("message", message.as_str())
            }),
        )?;
        f.member("id", ()) // null ID
    });
    response.to_string()
}
