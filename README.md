# bore
```shell
cargo run kafka-proxy --bootstrap-server localhost:9092
```

This will expose your local port and all the others retruned by kafka at `localhost:9092` to the public internet at `bore.pub:<PORT>`, where the port number is assigned randomly.



## Detailed Usage

This section describes detailed usage for the `bore` CLI command.

### Local Forwarding

```shell
cargo run kafka-proxy --bootstrap-server localhost:9092
```
The full options are shown below.

```shell
Starts a local LocalProxy to the remote server

Usage: bore kafka-proxy [OPTIONS]

Options:
  -b, --bootstrap-server <BOOTSTRAP_SERVER>  The local host to expose [default: localhost:9092]
  -s, --secret <SECRET>                      Optional secret for authentication [env: BORE_SECRET]
  -h, --help                                 Print help information
```

### Self-Hosting

As mentioned in the startup instructions, there is a public instance of the `bore` server running at `bore.pub`. However, if you want to self-host `bore` on your own network, you can do so with the following command:

```shell
bore server
```

That's all it takes! After the server starts running at a given address, you can then update the `bore local` command with option `--to <ADDRESS>` to forward a local port to this remote server.

The full options for the `bore server` command are shown below.

```shell
Runs the remote proxy server

Usage: bore server [OPTIONS]

Options:
      --min-port <MIN_PORT>  Minimum TCP port number to accept [default: 1024]
  -s, --secret <SECRET>      Optional secret for authentication [env: BORE_SECRET]
  -h, --help                 Print help information
```

## Protocol

There is an implicit _control port_ at `7835`, used for creating new connections on demand. At initialization, the client sends a "Hello" message to the server on the TCP control port, asking to proxy a selected remote port. The server then responds with an acknowledgement and begins listening for external TCP connections.

Whenever the server obtains a connection on the remote port, it generates a secure [UUID](https://en.wikipedia.org/wiki/Universally_unique_identifier) for that connection and sends it back to the client. The client then opens a separate TCP stream to the server and sends an "Accept" message containing the UUID on that stream. The server then proxies the two connections between each other.

For correctness reasons and to avoid memory leaks, incoming connections are only stored by the server for up to 10 seconds before being discarded if the client does not accept them.

## Authentication

On a custom deployment of `bore server`, you can optionally require a _secret_ to prevent the server from being used by others. The protocol requires clients to verify possession of the secret on each TCP connection by answering random challenges in the form of HMAC codes. (This secret is only used for the initial handshake, and no further traffic is encrypted by default.)

```shell
# on the server
bore server --secret my_secret_string

# on the client
bore local <LOCAL_PORT> --to <TO> --secret my_secret_string
```

If a secret is not present in the arguments, `bore` will also attempt to read from the `BORE_SECRET` environment variable.

## Acknowledgements

Created by Eric Zhang ([@ekzhang1](https://twitter.com/ekzhang1)). Licensed under the [MIT license](LICENSE).

The author would like to thank the contributors and maintainers of the [Tokio](https://tokio.rs/) project for making it possible to write ergonomic and efficient network services in Rust.
