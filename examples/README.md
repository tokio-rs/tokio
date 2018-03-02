## Examples of how to use Tokio

This directory contains a number of examples showcasing various capabilities of
the `tokio` crate.

All examples can be executed with:

```
cargo run --example $name
```

A high level description of each example is:

* `hello_world` - a tiny server that writes "hello world" to all connected
  clients and then terminates the connection, should help see how to create and
  initialize `tokio`.

* `echo` - this is your standard TCP "echo server" which accepts connections and
  then echos back any contents that are read from each connected client.

* `echo-udp` - again your standard "echo server", except for UDP instead of TCP.
  This will echo back any packets received to the original sender.

* `connect` - this is a `nc`-like clone which can be used to interact with most
  other examples. The program creates a TCP connection or UDP socket to sends
  all information read on stdin to the remote peer, displaying any data received
  on stdout. Often quite useful when interacting with the various other servers
  here!

* `chat` - this spins up a local TCP server which will broadcast from any
  connected client to all other connected clients. You can connect to this in
  multiple terminals and use it to chat between the terminals.

* `chat-combinator` - Similar to `chat`, but this uses a much more functional
  programming approch using combinators.

* `proxy` - an example proxy server that will forward all connected TCP clients
  to the remote address specified when starting the program.

* `tinyhttp` - a tiny HTTP/1.1 server which doesn't support HTTP request bodies
  showcasing running on multiple cores, working with futures and spawning
  tasks, and finally framing a TCP connection to discrete request/response
  objects.

* `tinydb` - an in-memory database which shows sharing state between all
  connected clients, notably the key/value store of this database.

If you've got an example you'd like to see here, please feel free to open an
issue. Otherwise if you've got an example you'd like to add, please feel free
to make a PR!
