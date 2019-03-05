use futures::{future, Future, Sink, Stream, Poll, Async, AsyncSink, StartSend, task};
use std::mem;
use std::env::args;
use std::process::exit;
use tokio::runtime::current_thread::Runtime;
use tokio_xmpp::{Client, Packet, Event};
use xmpp_parsers::{Jid, Element, TryFrom};
use xmpp_parsers::message::{Body, Message, MessageType};
use xmpp_parsers::presence::{Presence, Show as PresenceShow, Type as PresenceType};

use std::collections::HashMap;

fn main() {
    let args: Vec<String> = args().collect();
    if args.len() != 3 {
        println!("Usage: {} <jid> <password>", args[0]);
        exit(1);
    }

    let jid = &args[1];
    let password = &args[2];

    let mut rt = Runtime::new().unwrap();

    let client = Client::new(jid, password).unwrap();

    let (sink, stream) = client.split();

    let (mut tx, rx) = futures::unsync::mpsc::unbounded();
    let tx2 = tx.clone();
    rt.spawn(
        rx.forward(
            sink.sink_map_err(|_| panic!("Pipe"))
        )
            .map(|(rx, mut sink)| {
                drop(rx);
                let _ = sink.close();
            })
            .map_err(|e| {
                panic!("Send error: {:?}", e);
            })
    );

    let filtered = stream.filter_map(move |event| {
        if event.is_online() {
            println!("Online");

            let presence = make_presence();
            tx.start_send(Packet::Stanza(presence)).unwrap();

            None
        } else if let Event::Stanza(s) = event.clone() {
            if s.name() == "presence" && s.attr("from") == Some("botspam@chat.paranoidlabs.org") {
                println!("{:?}", s);
            }
            Some(event)
        } else {
            Some(event)
        }
    });

    let (psink, pstream) = ModuleHandler::new().split();

    let one = filtered.map_err(|_| {}).forward(psink).map(|_| {});
    let two = pstream.map(|e| Packet::Stanza(e)).forward(tx2.sink_map_err(|_| {})).map(|_| {});

    rt.spawn(one);
    rt.block_on(two);
}

fn make_presence() -> Element {
    let mut presence = Presence::new(PresenceType::None);
    presence.show = PresenceShow::Chat;
    presence.statuses
        .insert(String::from("en"), String::from("Echoing messages."));
    presence.into()
}

struct Channel {
    members: HashMap<String, Jid>,

}

trait Module<T>: Sink<SinkItem = Event, SinkError = ()> + Stream<Item = Element, Error = ()> {
}

struct ModuleHandler {
    sink: Box<Sink<SinkItem = Event, SinkError = ()>>,
    stream: Box<Stream<Item = Element, Error = ()>>,
}
impl ModuleHandler {
    pub fn new() -> ModuleHandler {
        let (esink, estream) = Echo::new().split();
        let (hsink, hstream) = Hello::new().split();
        let (jsink, jstream) = Join::new().split();
        ModuleHandler {
            sink: Box::new(esink
                   .fanout(hsink)
                   .fanout(jsink)
            ),
            stream: Box::new(estream
                     .select(hstream)
                     .select(jstream)
            ),
        }
    }
}

impl Module<()> for ModuleHandler {
}
impl Sink for ModuleHandler {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.sink.start_send(item)
    }
    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.sink.poll_complete()
    }
}
impl Stream for ModuleHandler {
    type Item = Element;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.stream.poll()
    }
}

enum Hello {
    Greeting(Jid),
    Waiting,
}
impl Hello {
    pub fn new() -> Self {
        Hello::Waiting
    }
}
impl Module<()> for Hello { }

impl Sink for Hello {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match *self {
            Hello::Waiting => {
                if let Some(message) = item.into_stanza().and_then(|stanza| Message::try_from(stanza).ok()) {
                    match (message.from, message.bodies.get("")) {
                        (Some(ref from), Some(ref body)) => {
                            if body.0.starts_with("^hello") {
                                *self = Hello::Greeting(from.clone());
                                task::current().notify();
                            }
                        },
                        _ => {}
                    }
                }
                Ok(AsyncSink::Ready)
            },
            Hello::Greeting(_) => Ok(AsyncSink::NotReady(item))
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        match *self {
            Hello::Waiting => Ok(Async::Ready(())),
            Hello::Greeting(_) => {
                task::current().notify();
                Ok(Async::NotReady)
            }
        }
    }
}
impl Stream for Hello {
    type Item = Element;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match mem::replace(self, Hello::Waiting) {
            Hello::Greeting(j) => {
                let mut message = Message::new(Some(j));
                message.bodies.insert(String::new(), Body("Hai >^.^<".into()));
                Ok(Async::Ready(Some(message.into())))
            },
            Hello::Waiting => Ok(Async::NotReady)
        }
    }
}

#[derive(Debug)]
enum Echo {
    Storing(Jid, String),
    Empty,
}

impl Echo {
    pub fn new() -> Echo {
        Echo::Empty
    }
}

impl Module<()> for Echo { }

impl Sink for Echo {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match *self {
            Echo::Empty => {
                if let Some(message) = item.into_stanza().and_then(|stanza| Message::try_from(stanza).ok()) {
                    match (message.from, message.bodies.get("")) {
                        (Some(ref from), Some(ref body)) => {
                            if body.0.starts_with("^echo") {
                                if body.0.len() < 7 {
                                    *self = Echo::Storing(from.clone(), "You need to tell me what to echo.".into());
                                } else {
                                    println!("I got a message {} from {}", body.0, from);
                                    *self = Echo::Storing(from.clone(), (body.0)[6..].to_owned());
                                }
                                task::current().notify();
                                return Ok(AsyncSink::Ready)
                            }
                        },
                        _ => {}
                    }
                }
                Ok(AsyncSink::Ready)
            },
            Echo::Storing(_,_) => Ok(AsyncSink::NotReady(item)),
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        match *self {
            Echo::Empty => Ok(Async::Ready(())),
            Echo::Storing(_,_) => {
                task::current().notify();

                Ok(Async::NotReady)
            }
        }
    }
}

impl Stream for Echo {
    type Item = Element;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match mem::replace(self, Echo::Empty) {
            Echo::Storing(j,s) => {
                let mut message = Message::new(Some(j));
                message.bodies.insert(String::new(), Body(s));
                Ok(Async::Ready(Some(message.into())))
            },
            Echo::Empty => Ok(Async::NotReady)
        }
    }
}

enum Join {
    Joining(Jid),
    Waiting,
}

impl Join {
    pub fn new() -> Self {
        Join::Waiting
    }
}

impl Module<()> for Join { }
impl Sink for Join {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match *self {
            Join::Waiting => {
                if let Some(message) = item.into_stanza().and_then(|stanza| Message::try_from(stanza).ok()) {
                    match (message.from, message.bodies.get("")) {
                        (Some(ref _from), Some(ref body)) => {
                            if body.0.starts_with("^join ") {
                                let mut chan = (body.0)[6..].split_whitespace();
                                if let Some(n) = chan.next() {
                                    if let Some(d) = chan.next() {
                                        let j = Jid::full(n,d,"gnutbot");
                                        println!("Joining {}", j);
                                        *self = Join::Joining(j);
                                        task::current().notify();
                                    }
                                }
                            }
                        },
                        _ => {}
                    }
                }
                Ok(AsyncSink::Ready)
            },
            Join::Joining(_) => Ok(AsyncSink::NotReady(item))
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        match *self {
            Join::Waiting => Ok(Async::Ready(())),
            Join::Joining(_) => {
                task::current().notify();
                Ok(Async::NotReady)
            }
        }
    }
}
impl Stream for Join {
    type Item = Element;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match mem::replace(self, Join::Waiting) {
            Join::Joining(j) => {
                let mut presence = Presence::new(PresenceType::None);
                presence.show = PresenceShow::None;
                presence.to = Some(j);
                Ok(Async::Ready(Some(presence.into())))
            },
            Join::Waiting => Ok(Async::NotReady)
        }
    }
}
