use failure::{format_err, Error};
use futures::{future::Future, prelude::*, sink::Sink, stream::Stream, Async};
use log::{info, warn};
use std::{
    ops::Add,
    time::{Duration, Instant},
};
use tokio::timer::Delay;
use websocket::{
    client::r#async::{Client, ClientNew},
    stream::r#async::Stream as WsStream,
    ClientBuilder, OwnedMessage,
};

pub mod protocol;

pub use protocol::*;

type WsClient = ClientNew<Box<WsStream + Send>>;
type WsClientFrameStream = Client<Box<WsStream + Send>>;

fn connect_to_ws(url: &str) -> Result<WsClient, Error> {
    Ok(ClientBuilder::new(url)?.async_connect(None))
}

pub struct SocketIoStream {
    heartbeat: Heartbeat,
    ws_stream: WsClientFrameStream,
    last_pong: Instant,
}

impl SocketIoStream {
    pub fn new(
        url: &str,
    ) -> Result<impl Future<Item = SocketIoStream, Error = failure::Error>, Error> {
        let res = connect_to_ws(url)?
            .map_err(|e| format_err!("{:?}", e))
            .and_then(|(ws, _)| Ok(SocketIoStream::with_ws_stream(ws)));
        Ok(res)
    }
    fn with_ws_stream(ws_stream: WsClientFrameStream) -> Self {
        Self {
            ws_stream,
            heartbeat: Heartbeat::default(),
            last_pong: Instant::now(),
        }
    }
    fn receive_pong(&mut self) {
        info!("receive pong");
        self.last_pong = Instant::now();
    }
    fn is_pong_ok(&self) -> bool {
        let now = Instant::now();
        let res = now - self.last_pong;
        let res_bool = res <= Duration::from_secs(10);
        info!(
            "is_pong_ok? {:?} {:?} res {:?} is_ok {}",
            now, self.last_pong, res, res_bool
        );
        return res_bool;
    }
}

fn build_delay(dur: Duration) -> Delay {
    let deadline = Instant::now().add(dur);
    return tokio::timer::Delay::new(deadline);
}

struct Heartbeat {
    delay: Option<Delay>,
    state: HeartbeatState,
}

impl Default for Heartbeat {
    fn default() -> Self {
        Self {
            delay: None,
            state: HeartbeatState::Start,
        }
    }
}

#[derive(Debug)]
enum HeartbeatState {
    Start,
    DelayPing,
    DelayPong,
}

#[derive(Debug)]
enum HeartBeatEvent {
    ShouldPing,
    ShouldPong,
}

impl Heartbeat {
    fn pong_over(&mut self) {
        self.state = HeartbeatState::Start;
        self.delay = None;
    }
    fn prepare_ping(&mut self) {
        match self.state {
            HeartbeatState::Start => {
                self.state = HeartbeatState::DelayPing;
                self.delay = Some(build_delay(Duration::from_secs(3)));
            }
            HeartbeatState::DelayPing => {
                warn!("Ping Again?");
            }
            HeartbeatState::DelayPong => {
                warn!("Dont Ping When Not Pong");
            }
        }
    }

    fn ping_over(&mut self) {
        info!("switch wait check pong");
        self.state = HeartbeatState::DelayPong;
        self.delay = Some(build_delay(Duration::from_secs(3)));
    }
}

impl Stream for Heartbeat {
    type Item = HeartBeatEvent;
    type Error = failure::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        if let Some(delay) = &mut self.delay {
            match delay.poll() {
                Ok(Async::Ready(())) => {
                    self.delay = None;
                    match self.state {
                        HeartbeatState::DelayPing => {
                            return Ok(Async::Ready(Some(HeartBeatEvent::ShouldPing)));
                        }
                        HeartbeatState::DelayPong => {
                            return Ok(Async::Ready(Some(HeartBeatEvent::ShouldPong)));
                        }
                        _ => {
                            return Err(format_err!("why this? state start and has delay?"));
                        }
                    }
                }
                Ok(Async::NotReady) => {
                    return Ok(Async::NotReady);
                }
                Err(e) => {
                    return Err(format_err!("err {:?}", e));
                }
            }
        }
        return Ok(Async::NotReady);
    }
}

impl Sink for SocketIoStream {
    type SinkItem = SocketIoEvent;
    type SinkError = Error;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        match self
            .ws_stream
            .start_send(OwnedMessage::Text(item.encode()))?
        {
            AsyncSink::Ready => Ok(AsyncSink::Ready),
            AsyncSink::NotReady(_) => Ok(AsyncSink::NotReady(item)),
        }
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        self.ws_stream.poll_complete().map_err(|e| e.into())
    }

    fn close(&mut self) -> Result<Async<()>, Self::SinkError> {
        self.ws_stream.close().map_err(|e| e.into())
    }
}

impl Stream for SocketIoStream {
    type Item = SocketIoEvent;
    type Error = Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        info!("ss poll");
        match self.heartbeat.poll() {
            Ok(Async::Ready(Some(HeartBeatEvent::ShouldPing))) => {
                info!("should ping");
                let _ = self
                    .ws_stream
                    .start_send(OwnedMessage::Text("2".to_string()));
                self.heartbeat.ping_over();
            }
            Ok(Async::Ready(Some(HeartBeatEvent::ShouldPong))) => {
                info!("should check pong");
                if self.is_pong_ok() {
                    self.heartbeat.pong_over();
                    self.heartbeat.prepare_ping();
                } else {
                    warn!("not receive a pong");
                }
            }
            Ok(Async::Ready(None)) => {
                info!("no more heartbeat? should not even happen");
            }

            Ok(Async::NotReady) => {}
            Err(_) => {}
        }

        match self.ws_stream.poll() {
            Ok(Async::Ready(ws_event)) => {
                //println!("ws event {:?}", ws_event);
                match ws_event {
                    Some(OwnedMessage::Text(txt)) => match SocketIoEvent::parse(txt) {
                        Ok(SocketIoEvent::Open) => {
                            self.heartbeat.prepare_ping();
                            return Ok(Async::Ready(Some(SocketIoEvent::Open)));
                        }
                        Ok(SocketIoEvent::Pong) => {
                            self.receive_pong();
                        }
                        Ok(SocketIoEvent::Event(event)) => {
                            return Ok(Async::Ready(Some(SocketIoEvent::Event(event))));
                        }

                        Ok(other) => info!("socket io event from server {:?}", other),

                        Err(_) => {}
                    },
                    Some(OwnedMessage::Binary(_)) => {}
                    Some(OwnedMessage::Ping(_)) => {}
                    Some(OwnedMessage::Pong(_)) => {}
                    Some(OwnedMessage::Close(_)) => {}
                    None => {}
                }
            }
            Ok(Async::NotReady) => {}
            Err(e) => {
                info!("ws err {:?}", e);
            }
        };
        Ok(Async::NotReady)
    }
}
