extern crate futures;
extern crate tokio;
extern crate websocket;

use failure::{Error, format_err};
use futures::{Async, sync::mpsc, stream::Stream, sink::Sink, future::{Future, IntoFuture}, prelude::*, try_ready, task};
use std::thread;
use websocket::{result::WebSocketError,
                ClientBuilder,
                OwnedMessage,
                client::r#async::{ClientNew, Client},
                stream::r#async::Stream as WsStream,
                header::Headers,
};
use tokio::{timer::Delay};
use std::time::{Instant, Duration};

use std::ops::Add;

use log::*;
use simplelog::*;

use std::fs::File;
use futures::task::current;


const URL: &'static str = "ws://localhost:3000/socket.io/?EIO=3&transport=websocket";

type WsClient = ClientNew<Box<WsStream + Send>>;
type WsClientFrameStream = Client<Box<WsStream + Send>>;

fn connect_to_ws(url: &str) -> Result<WsClient, Error> {
    Ok(ClientBuilder::new(url)?
        .async_connect(None))
}


#[derive(Debug)]
enum SocketIoEvent {
    Connect,
    Event(String),
    DisConnect,
    Ping,
    TimeOut,
    ReConnect,
    Close,
}


struct WsClientWrapper {
    ws_client: WsClient,
    msg_stream: Option<(WsClientFrameStream, Headers)>,
}

impl WsClientWrapper {
    fn new(url: &str) -> Result<WsClientWrapper, Error> {
        Ok(Self { ws_client: connect_to_ws(url)?, msg_stream: None })
    }
    fn ping(&mut self) {
        if let Some((stream, _)) = &mut self.msg_stream {
            stream.send(OwnedMessage::Text("2".to_string()));
        }
    }
}

impl Stream for WsClientWrapper {
    type Item = OwnedMessage;
    type Error = Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        if let Some((msg_stream, _header)) = &mut self.msg_stream {
            return msg_stream.poll().map_err(|e| e.into());
        } else {
            self.msg_stream = Some(try_ready!(self.ws_client.poll()));
            return Ok(Async::NotReady);
        }
    }
}

struct SocketIoStream {
    ws_client: WsClientWrapper,
    ping: Option<Delay>,
    pong: Option<Delay>,
}

impl SocketIoStream {
    fn from_ws_url(url: &str) -> Result<Self, Error> {
        Ok(Self {
            ws_client: WsClientWrapper::new(url)?,
            ping: None,
            pong: None,
        })
    }
}

impl Stream for SocketIoStream {
    type Item = SocketIoEvent;
    type Error = Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        info!("SocketIoStream poll");
        if let Some(ping) = &mut self.ping {
            let ping_res = ping.poll();
            info!("start call ping poll {:?}", ping_res);
            match ping_res {
                Ok(Async::Ready(())) => {
                    self.ws_client.ping();
                }
                _ => {}
            }
        }

        let ws_event = try_ready!(self.ws_client.poll());


        if ws_event.is_none() {
            //mean ws socket end
            return Ok(Async::Ready(None));
        }


        let _ = match ws_event.unwrap() {
            OwnedMessage::Binary(buff) => {}
            OwnedMessage::Close(data) => {
                info!("websocket close");
            }
            OwnedMessage::Ping(buff) => {}
            OwnedMessage::Pong(buff) => {}
            OwnedMessage::Text(txt_msg) => {
                match txt_msg.as_str() {
                    "40" => {
                        info!("socket io connect");
                        let deadline = Instant::now().add(Duration::from_secs(3));
                        let ping = tokio::timer::Delay::new(deadline);
                        current().notify();

                        info!("ping {:?}", ping);
                        self.ping = Some(ping);
                        task::current().notify();
                    }
                    msg => {
                        info!("websocket txt msg {}", msg);
                    }
                }
            }
        };
        Ok(Async::NotReady)
    }
}

fn run() -> Result<impl Future<Item=(), Error=failure::Error>, Error> {
    let res = SocketIoStream::from_ws_url(URL)?
        .for_each(|event| {
            println!("socket-io event {:?}", event);
            Ok(())
        }).into_future();
    Ok(res)
}

fn main() {
    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Info, Config::default()).unwrap(),
            WriteLogger::new(LevelFilter::Info, Config::default(), File::create("my_rust_binary.log").unwrap()),
        ]
    ).unwrap();

    info!("Connecting to {}", URL);
    let mut runtime = tokio::runtime::current_thread::Builder::new()
        .build()
        .unwrap();
    runtime.block_on(run().unwrap()).unwrap();
}