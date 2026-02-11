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

pub fn validate_json_rpc_request<'text, 'raw>(
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
