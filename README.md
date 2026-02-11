jlou
====

[![jlou](https://img.shields.io/crates/v/jlou.svg)](https://crates.io/crates/jlou)
[![Actions Status](https://github.com/sile/jlou/workflows/CI/badge.svg)](https://github.com/sile/jlou/actions)
![License](https://img.shields.io/crates/l/jlou)

This is a command-line tool for [JSON-RPC 2.0] over [JSON Lines] over UDP.

[JSON-RPC 2.0]: https://www.jsonrpc.org/specification
[JSON Lines]: https://jsonlines.org/

```console
$ cargo install jlou

$ jlou -h
Command-line tool for JSON-RPC 2.0 over JSON Lines over UDP

Usage: jlou [OPTIONS] <COMMAND>

Commands:
  req         Generate a JSON-RPC request object JSON
  call        Read JSON-RPC requests from standard input and execute the RPC calls
  echo-server Run a JSON-RPC echo server

Options:
      --version Print version
  -h, --help    Print help ('--help' for full help, '-h' for summary)
```

Examples
--------

### Basic RPC call

Start an echo server in a terminal (":9000" is shorthand for "127.0.0.1:9000"):
```console
$ jlou echo-server :9000
```

Execute an RPC call in another terminal:
```console
$ jlou req hello --params '["world"]' | jlou call :9000 --pretty
{
  "jsonrpc": "2.0",
  "result": {
    "id": 0,
    "jsonrpc": "2.0",
    "method": "hello",
    "params": [
      "world"
    ]
  },
  "id": 0
}
```

UDP
---

`jlou` uses UDP for both requests and responses. Each UDP packet may contain
multiple JSON Lines joined with `\n` up to `--send-buf-size` (default: 1200).
Responses must fit in a single UDP packet. Tune `--send-buf-size` on both
`call` and `echo-server` if you need larger payloads.
