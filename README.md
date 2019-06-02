# SocketIo-Client-Rust
socketio-client-rust is rust implement of [socket.io-client](https://github.com/socketio/socket.io-client),which treat socket.io stream as future stream. 

# How to use
you could refer to examples/socketio_cli. to run this example you must launch a socket.io server under port 3000.
```rust
//some code

fn run() -> impl Future<Item=(), Error=failure::Error> {
    const URL: &'static str = "ws://localhost:3000/socket.io/?EIO=3&transport=websocket";

    let runner = SocketIoStream::new(URL)
        .unwrap()
        .and_then(|ss: SocketIoStream| {
            let (sink, ss_stream) = ss.split();
            let f1 = socket_io_control_stream().forward(sink).map(|(_, _)| ());
            let f2 = ss_stream.for_each(|e| {
                println!("event {:?}", e);
                Ok(())
            });
            let f = f1.join(f2).map(|(_, _)| ());
            return f;
        });
    return runner;
}
```