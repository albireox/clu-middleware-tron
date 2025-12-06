# clu-middleware-tron

`clu-middleware-tron` is a middleware service and library that facilitates communication between a legacy actor (Tron-style) and [new-style actors](https://github.com/sdss/clu) using RabbitMQ as the message broker. It is meant as a temporary solution to support legacy actors that are unlikely to be updated to CLU/RabbitMQ in the near future (`tcc`, `mcp`, `apogee`, `apogeecal`, etc.)

`clu-middleware-tron` is primarily a runnable binary, but a library is also provided with the underlying tools. The documentation for the library can be found [here](https://sdss.github.io/clu-middleware-tron/clu_middleware_tron/). The crate is not published to `crates.io` since it is only meant for internal use, but you can add it as a dependency in your `Cargo.toml` using:

```toml
clu-middleware-tron = { git = "https://github.com/sdss/clu-middleware-tron.git" }
```

## Basic usage

To use `clu-middleware-tron` with a Tron actor called `sample-actor` running on port 9000, run the following command:

```sh
clu-middleware-tron -H 127.0.0.1 -p 9000 --reconnect start sample-actor
```

This assumes that RabbitMQ is running on `localhost` with default port and credentials. You can customize the RabbitMQ connection parameters by passing a `--rabbitmq-uri` argument. It is also possible to use environment variables to configure the Tron host/port and RabbitMQ connection parameters. By default, `clu-middleware-tron` will use the default exchange for CLU actors, `sdss_exchange`. Run `clu-middleware-tron --help` for more information.

## Communication model

What follows is a short description of the Tron and CLU/AMQP communication models for the purposes of understanding `clu-middleware-tron`. For a more comprehensive overview see the [CLU documentation](https://clu.readthedocs.io/en/latest/).

Tron-style actors are TCP servers that listen for commands in the form `[<command_id>] <command_string>` where `<command_id>` is an integer that identifies the command and `<command_string>` is the string describing the command and its arguments. The `command_id` is used to match responses to commands and is optional (if omitted the replies use `command_id=0`). When a client connects to the actor assigns that socket an integer `user_id`. When a command arrives the actor processes its command string and asynchronously calls a backend function with the appropriate arguments. The backend can return information as "replies" which follow the form `<user_id> <command_id> <code> [<keywords>]` where `<code>` is a character that defines the type of message (`i` for informational, `d` for debug, etc.) The `<code>` can also be `:` or `f` indicating that the command backend has finished running with status successful or failed, respectively. A command must, at least, emit one reply with the final status code. The `<keywords>` consist of semi-colon-separated key-value pairs, for example `name=john;age=42`. The keyword specification allows sending numbers, booleans, strings, lists, nulls, etc. The format is non-standard so a special parser is required to convert keywords into a proper structure. When a Tron actor replies, it usually does so only in the socket of the client that sent the command. However, an actor can choose to "broadcast" replies to all connected clients, in which case the `user_id` is set to 0 in the reply.

Tron provides the message passing system for Tron-style actors. The Tron process open a client socket to each actor in the system. Users connect to Tron on a specific port (6093 for unsecured connections, 9877 for secured ones) and send commands to the actors with strings of the form `<commander_id> <command_id> <actor> <command_string>` where `<commander_id>` is of the form `apo.john` and can be used to track child commands. Tron then takes sends the command to `<actor>` with a different `<command_id>` that is guaranteed to be unique, to avoid potential conflicts. When the actor replies Tron performs the inverse operation, reattaches the `commander_id` and original `command_id` and outputs it to *all* the users. Actors can optionally connect to Tron as clients. This allows actors to command other actors and to listen to the replies from other actors. In practice Tron does some additional things, but those are beyond this scope.

CLU/AMQP actors use RabbitMQ (or other AMQP exchange) as the message broker and have two main advantages: the replies are properly serialised JSON strings, and the message broker is a standard service that requires no configuration. All communication happens in a dedicated AMQP [topic exchange](https://www.rabbitmq.com/tutorials/tutorial-five-python), `sdss_exchange`. When started, CLU actors connect to RabbitMQ and declare the exchange and a queue to receive message with correlation ID `command.<actor>.#`, i.e., all commands directed to the actor. The actor then processes the command and can reply with messages that are published to the exchange with a routing key starting with `reply` (`reply.<commander_id>` when replying to a command from a commander, or `reply.broadcast` when broadcasting to all clients). An actor or client also declares a queue bound to messages with correlation ID `reply.#` which allows them to receive all replies emitted by any actor (clients could only listen to replies directed to them, but in practice we receive all message and decide whether to act on them later).

Command messages sent to the exchange have the following components:

- A body which contains the serialised structure `{"command_string": <command_string>}` with the command to execute.
- A set of headers including the `command_id` (a unique UUID of the form `7cbad46a-85cf-4019-a4d5-b81e0c54e138` which is generated by the client), `commander_id` (usually of the form `<sender>.<actor>` where `<sender>` is the name of the client sending the command and `<actor>` is the actor to receive it), and some options to determine how the replies should be processed.
- A correlation ID which is equal to the `command_id` UUID.
- A routing key of the form `command.<actor>`.

Replies from the actor have these components:

- A body which is a serialised, JSON-compliant string.
- A set of headers including at least `message_code` (`i`, `d`, `w`, `f`, `:`, etc., inherited from the Tron-style messages), `commander_id`, `command_id`, and `sender` (the name of the client that commanded the actor).
- A correlation ID equal to `command_id`.
- A routing key equal to `reply.<commander_id>` for replies to a client, or `reply.broadcast` for broadcasts. In practice all clients receive all messages but the routing key can be used to reject broadcasts or messages meant for other clients.

## How `clu-middleware-tron` works

`clu-middleware-tron` provides a translation service between the two models described above. When an instance starts, it creates two tasks running concurrently. One of the tasks establishes a TCP connection to the Tron-like actor, while the other connects to the RabbitMQ exchange. Communication between the threads is accomplished using a [multiple producer, multiple consumer channel](https://docs.rs/async-channel/latest/async_channel/fn.unbounded.html) (MPMC).

The RabbitMQ connection listens for commands in a queue bound to the `command.<actor>` topic. When a message is received, it puts it in the MPMC queue to be processed by the RabbitMQ task. That TCP task receives the command, formats it as a Tron command, and sends it to the actor. Since the command ID and commander formats are different, a mapping of command IDs is maintained and the system handles the translation between UUID and numeric format. The TCP task also listens for replies from the actor, parses the bytes array, and creates a reply structure with the keywords and command metadata. The reply structure is then passed to the RabbitMQ task using another MPMC queue, processed, and published to the exchange with the appropriate routing key, correlation ID, and headers.

One instance of `clu-middleware-tron` is required for each Tron-style actor. This "atomic" design was selected over a more Tron-like service that could support a group of legacy actors in a single process.

## Limitations

There are a number of known limitations to `clu-middleware-tron`:

- The middleware does *not* provide a system for Tron-style actors to command other actors over RabbitMQ or listen to other actor's replies. In practice this would require rebuilding a minimal version of Tron as a message passing system and was considered unnecessary. None of the actors that are likely to use this middleware use Tron to command or monitor other actors.
- The RabbitMQ service must be running when the middleware stars. Reconnection to RabbitMQ has not yet been implemented. Reconnection to the actor via TCP can happen if the `--reconnect` flag is used. In practice, it's rare for the RabbitMQ service to be restarted but the actors often are.
