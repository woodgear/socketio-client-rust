extern crate socketio_client_rust;

use failure::format_err;
use futures::sync::mpsc::channel;
use futures::{future::Future, sink::Sink, stream::Stream};
use socketio_client_rust::*;
use std::{
    io::{self, BufRead},
    process, thread,
};

fn stdin_stream() -> impl Stream<Item = String, Error = ()> {
    let (mut tx, rx) = channel(1);
    thread::spawn(move || {
        for line in io::stdin().lock().lines().filter_map(|line| line.ok()) {
            tx = tx.send(line).wait().unwrap();
        }
    });

    rx
}

fn socket_io_control_stream() -> impl Stream<Item = SocketIoEvent, Error = failure::Error> {
    stdin_stream()
        .map_err(|_| format_err!(""))
        .filter_map(|input| {
            if input.is_empty() {
                return None;
            }
            let mut split = input.split_whitespace();
            let event_name = split.next().unwrap();
            if event_name.eq("/exit") {
                process::exit(0);
            }
            let event_context: String = split.collect();
            Some(SocketIoEvent::Event(CustomStringEvent::new(
                event_name,
                event_context,
            )))
        })
}

fn run() -> impl Future<Item = (), Error = failure::Error> {
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

fn main() {
    let mut runtime = tokio::runtime::current_thread::Builder::new()
        .build()
        .unwrap();
    runtime.block_on(run()).unwrap();
}
