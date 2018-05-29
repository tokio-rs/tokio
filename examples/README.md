## Examples of how to use Tokio

This directory contains a number of examples showcasing various capabilities of
the `tokio` crate.

All examples can be executed with:

```
cargo run --example $name
```

A high level description of each example is:

* [`hello_world`](hello_world.rs) - a tiny server that writes "hello world" to
  all connected clients and then terminates the connection, should help see how
  to create and initialize `tokio`.

* [`echo`](echo.rs) - this is your standard TCP "echo server" which accepts
  connections and then echos back any contents that are read from each connected
  client.

* [`print_each_packet`](print_each_packet.rs) - this server will create a TCP
  listener, accept connections in a loop, and put down in the stdout everything
  that's read off of each TCP connection.

* [`echo-udp`](echo-udp.rs) - again your standard "echo server", except for UDP
  instead of TCP.  This will echo back any packets received to the original
  sender.

* [`connect`](connect.rs) - this is a `nc`-like clone which can be used to
  interact with most other examples. The program creates a TCP connection or UDP
  socket to sends all information read on stdin to the remote peer, displaying
  any data received on stdout. Often quite useful when interacting with the
  various other servers here!

* [`chat`](chat.rs) - this spins up a local TCP server which will broadcast from
  any connected client to all other connected clients. You can connect to this
  in multiple terminals and use it to chat between the terminals.

* [`chat-combinator`](chat-combinator.rs) - Similar to `chat`, but this uses a
  much more functional programming approach using combinators.

* [`proxy`](proxy.rs) - an example proxy server that will forward all connected
  TCP clients to the remote address specified when starting the program.

* [`tinyhttp`](tinyhttp.rs) - a tiny HTTP/1.1 server which doesn't support HTTP
  request bodies showcasing running on multiple cores, working with futures and
  spawning tasks, and finally framing a TCP connection to discrete
  request/response objects.

* [`tinydb`](tinydb.rs) - an in-memory database which shows sharing state
  between all connected clients, notably the key/value store of this database.

* [`udp-client`](udp-client.rs) - a simple `send_dgram`/`recv_dgram` example.

* [`manual-runtime`](manual-runtime.rs) - manually composing a runtime.

If you've got an example you'd like to see here, please feel free to open an
issue. Otherwise if you've got an example you'd like to add, please feel free
to make a PR!
