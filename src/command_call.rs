use std::io::{BufRead, Write};
use std::net::UdpSocket;
use std::time::Duration;

const MAX_UDP_PACKET: usize = 65507;
const DEFAULT_BUF_SIZE_STR: &str = "1200";
const DEFAULT_TIMEOUT_MS_STR: &str = "5000";

pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
    if !noargs::cmd("call")
        .doc("Read JSON-RPC requests from standard input and execute the RPC calls (UDP only)")
        .take(args)
        .is_present()
    {
        return Ok(false);
    }

    let server_addr: String = noargs::arg("<SERVER>")
        .doc("JSON-RPC server address or hostname")
        .example("127.0.0.1:8080")
        .take(args)
        .then(|a| -> Result<_, String> { Ok(normalize_server_addr(a.value())) })?;
    let pretty: bool = noargs::flag("pretty")
        .short('p')
        .doc("Pretty-print JSON responses")
        .take(args)
        .is_present();
    let buf_size: usize = noargs::opt("buf-size")
        .ty("BYTES")
        .doc("Maximum UDP payload size per packet (bytes)")
        .default(DEFAULT_BUF_SIZE_STR)
        .take(args)
        .then(|o| o.value().parse())?;
    let timeout_ms: u64 = noargs::opt("timeout")
        .ty("MILLISECONDS")
        .doc("Timeout in milliseconds for waiting responses")
        .default(DEFAULT_TIMEOUT_MS_STR)
        .take(args)
        .then(|o| o.value().parse())?;

    if args.metadata().help_mode {
        return Ok(true);
    }

    let call_command = CallCommand {
        server_addr,
        pretty,
        buf_size,
        timeout: Duration::from_millis(timeout_ms),
    };
    call_command.run()?;

    Ok(true)
}

struct CallCommand {
    server_addr: String,
    pretty: bool,
    buf_size: usize,
    timeout: Duration,
}

impl CallCommand {
    fn run(self) -> crate::Result<()> {
        if self.buf_size == 0 {
            return Err(crate::Error::new("buf-size must be greater than 0"));
        }
        if self.buf_size > MAX_UDP_PACKET {
            return Err(crate::Error::new(format!(
                "buf-size must be <= {MAX_UDP_PACKET}"
            )));
        }
        if self.timeout == Duration::from_millis(0) {
            return Err(crate::Error::new("timeout must be greater than 0"));
        }

        let socket = self.connect_to_server_udp()?;
        socket.set_read_timeout(Some(self.timeout))?;

        let stdin = std::io::stdin();
        let input_reader = std::io::BufReader::new(stdin.lock());
        let stdout = std::io::stdout();
        let mut output_writer = std::io::BufWriter::new(stdout.lock());

        let mut send_buf: Vec<u8> = Vec::with_capacity(self.buf_size);
        let mut pending_responses = 0usize;

        for line in input_reader.lines() {
            let line = line?;
            let request = Request::parse(line)?;
            let request_text = request.json.text();
            let request_len = request_text.as_bytes().len();

            if request_len > self.buf_size {
                return Err(crate::Error::new(
                    "request size exceeds buf-size",
                ));
            }

            let extra = if send_buf.is_empty() { 0 } else { 1 };
            if send_buf.len() + extra + request_len > self.buf_size {
                self.flush_send_buf(&socket, &mut send_buf)?;
            }

            if !send_buf.is_empty() {
                send_buf.push(b'\n');
            }
            send_buf.extend_from_slice(request_text.as_bytes());

            if request.id.is_some() {
                pending_responses += 1;
            }
        }

        if !send_buf.is_empty() {
            self.flush_send_buf(&socket, &mut send_buf)?;
        }

        if pending_responses > 0 {
            self.receive_responses(&socket, &mut output_writer, pending_responses)?;
        }

        output_writer.flush()?;
        Ok(())
    }

    fn connect_to_server_udp(&self) -> crate::Result<UdpSocket> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect(&self.server_addr)?;
        Ok(socket)
    }

    fn flush_send_buf(&self, socket: &UdpSocket, send_buf: &mut Vec<u8>) -> crate::Result<()> {
        let size = socket.send(send_buf)?;
        if size != send_buf.len() {
            return Err(crate::Error::new(
                "failed to send complete request packet",
            ));
        }
        send_buf.clear();
        Ok(())
    }

    fn receive_responses(
        &self,
        socket: &UdpSocket,
        output_writer: &mut impl Write,
        expected: usize,
    ) -> crate::Result<()> {
        let mut recv_buf = vec![0u8; self.buf_size];
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

            if bytes_read == 0 {
                continue;
            }

            let text = std::str::from_utf8(&recv_buf[..bytes_read])?;
            for line in text.lines() {
                if line.is_empty() {
                    continue;
                }
                let response = Response::parse(line.to_owned())?;
                self.write_response(output_writer, &response)?;
                received += 1;
            }
        }
        Ok(())
    }

    fn write_response(
        &self,
        output_writer: &mut impl Write,
        response: &Response,
    ) -> crate::Result<()> {
        if self.pretty {
            let pretty_json = nojson::json(|f| {
                f.set_indent_size(2);
                f.set_spacing(true);
                f.value(response.json.value())
            });
            writeln!(output_writer, "{}", pretty_json)?;
        } else {
            writeln!(output_writer, "{}", response.json)?;
        }
        Ok(())
    }
}

fn normalize_server_addr(s: &str) -> String {
    if s.starts_with(':') {
        format!("127.0.0.1{s}")
    } else {
        s.to_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum RequestId {
    Number(i64),
    String(String),
}

struct Request {
    json: nojson::RawJsonOwned,
    id: Option<RequestId>,
}

impl Request {
    fn parse(json_text: String) -> Result<Self, nojson::JsonParseError> {
        let json = nojson::RawJsonOwned::parse(json_text)?;
        let id = Self::validate_request_and_parse_id(json.value())?;
        Ok(Self { json, id })
    }

    fn validate_request_and_parse_id(
        value: nojson::RawJsonValue<'_, '_>,
    ) -> Result<Option<RequestId>, nojson::JsonParseError> {
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
                    id = match value.kind() {
                        nojson::JsonValueKind::Integer => {
                            Some(RequestId::Number(value.try_into()?))
                        }
                        nojson::JsonValueKind::String => Some(RequestId::String(value.try_into()?)),
                        _ => {
                            return Err(value.invalid("id must be an integer or string"));
                        }
                    };
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
}

struct Response {
    json: nojson::RawJsonOwned,
    _id: Option<RequestId>,
}

impl Response {
    fn parse(json_text: String) -> Result<Self, nojson::JsonParseError> {
        let json = nojson::RawJsonOwned::parse(json_text)?;
        let id = Self::validate_response_and_parse_id(json.value())?;
        Ok(Self { json, _id: id })
    }

    fn validate_response_and_parse_id(
        value: nojson::RawJsonValue<'_, '_>,
    ) -> Result<Option<RequestId>, nojson::JsonParseError> {
        if value.kind() == nojson::JsonValueKind::Array {
            return Err(value.invalid("batch responses are not supported"));
        }

        let mut has_jsonrpc = false;
        let mut id = None;
        let mut has_result_or_error = false;

        for (name, value) in value.to_object()? {
            match name.as_string_str()? {
                "jsonrpc" => {
                    if value.as_string_str()? != "2.0" {
                        return Err(value.invalid("jsonrpc version must be '2.0'"));
                    }
                    has_jsonrpc = true;
                }
                "id" => {
                    id = match value.kind() {
                        nojson::JsonValueKind::Integer => {
                            Some(RequestId::Number(value.try_into()?))
                        }
                        nojson::JsonValueKind::String => Some(RequestId::String(value.try_into()?)),
                        _ => return Err(value.invalid("id must be an integer or string")),
                    };
                }
                "result" | "error" => {
                    has_result_or_error = true;
                }
                _ => {
                    // Ignore unknown members
                }
            }
        }

        if !has_jsonrpc {
            return Err(value.invalid("jsonrpc field is required"));
        }
        if !has_result_or_error {
            return Err(value.invalid("result or error field is required"));
        }

        Ok(id)
    }
}
