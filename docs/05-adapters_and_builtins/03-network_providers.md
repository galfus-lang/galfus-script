# Network Providers

Galfus exposes network capabilities through small, host-provided bridge
modules. Their operations may suspend the calling virtual thread, while other
virtual threads and the host event loop continue to run.

| Module          | Native host | Web host | Purpose                        |
| --------------- | ----------- | -------- | ------------------------------ |
| `std/net`       | Yes         | No       | Raw TCP client and UDP sockets |
| `std/http`      | Yes         | Yes      | One HTTP request and response  |
| `std/websocket` | Yes         | Yes      | WebSocket client connections   |

The native host is the CLI/default native embedding. The web host is used by
the Wasm host and the web playground. Importing one of these modules requires
the corresponding provider to be installed by the execution host.

## `std/net`

`std/net` is intentionally a low-level native-only surface. It provides TCP
client connections and UDP datagrams, but no HTTP, TLS, server, DNS, or
connection-pooling utilities. Those higher-level utilities can build on this
surface later without changing its handle model.

```galfus
import { tcpClose, tcpConnect, tcpRead, tcpWrite } from "std/net"

export fn main(args: [[u8]]): i32 {
  const connection = tcpConnect("127.0.0.1", 9000)
  return instanceof connection {
    u64 socket {
      if !tcpWrite(socket, "ping") {
        tcpClose(socket)
        return 1
      }
      const reply = tcpRead(socket, 1024)
      tcpClose(socket)
      return 0
    },
    null => 1,
  }
}
```

All socket handles are `u64` IDs owned by the provider and valid only for the
current execution. `tcpRead` waits for data. `udpReceive` returns a tuple of
the received bytes, peer host bytes, and peer port. Close each successful TCP
or UDP handle with its matching close function.

Raw TCP and UDP APIs are not exposed on the web because browser JavaScript
does not provide arbitrary socket access.

## `std/http`

`std/http` provides one request at a time. It accepts byte strings so callers
can choose their own encoding and returns `(status, headers, body)`.

```galfus
import { request } from "std/http"

export fn main(args: [[u8]]): i32 {
  const response = request("GET", "https://example.com")
  if response == null {
    return 1
  }
  return 0
}
```

The native host performs HTTP requests directly. The web host uses the browser
Fetch API, so requests are subject to the page origin, CORS policy, mixed
content restrictions, and other browser networking rules. The current web
provider returns an empty response-header list.

## `std/websocket`

`std/websocket` is a message-oriented client API for `ws://` and `wss://` URLs.

```galfus
import { close, connect, receive, send } from "std/websocket"

export fn main(args: [[u8]]): i32 {
  const connection = connect("wss://example.com/socket")
  return instanceof connection {
    u64 socket {
      if send(socket, "hello") {
        const message = receive(socket)
      }
      close(socket)
      return 0
    },
    null => 1,
  }
}
```

`connect` returns a provider-owned `u64` ID after the connection opens, or
`null` if it cannot connect. `receive` waits for the next text or binary
message and returns `null` for an invalid or closed socket. `send` and `close`
report success with `bool`. Call `close` when the connection is no longer
needed.

In the web host, WebSocket URLs must also satisfy browser security rules; for
example, a secure page normally requires `wss://` rather than `ws://`.
