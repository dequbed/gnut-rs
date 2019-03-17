#[macro_use]
extern crate futures;

use futures::{Future, Sink, Stream, Poll, StartSend};
use futures::unsync::mpsc;
use std::env::args;
use std::process::exit;
use tokio::runtime::current_thread::Runtime;
use tokio_xmpp::{Client, Packet};
use xmpp_parsers::Element;
use xmpp_parsers::presence::{Presence, Show as PresenceShow, Type as PresenceType};

mod command;
use command::{CombinatorModule, StreamPlugin, FuturePlugin, Combinator};

mod pipes;
use pipes::{Pipes, ChannelHandler};

mod plugins;
use plugins::{Quotes, CommandM, Join, Snack};

mod into_stream;

mod sendable;
use sendable::SendMessage;

struct ExamplePlugin;
impl StreamPlugin for ExamplePlugin {
    fn call(&mut self, _e: SendMessage) -> Box<Stream<Item=Element, Error=()>> {
        use futures::stream;
        Box::new(stream::empty())
    }
}
struct ExampleFPlugin;
impl FuturePlugin for ExampleFPlugin {
    fn call(&mut self, _e: SendMessage) -> Box<Future<Item=Option<Element>, Error=()>> {
        use futures::future;
        Box::new(future::empty())
    }
}


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

    let (mut tx, rx) = futures::unsync::mpsc::channel(16);
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

    let quotes = Combinator::new(Box::new(Quotes::new(Vec::new())));
    let command = Combinator::new(Box::new(CommandM::new()));
    let join = Combinator::new(Box::new(Join::new()));
    let snack = Combinator::new(Box::new(Snack::new()));
    let (cmi, cms) = CombinatorModule::new_with_modules(vec![quotes, command, join, snack]).split();

    let dm = ChannelHandler::new_with_modules(vec![Box::new(cmi)], vec![Box::new(cms)]);
    let (iqtx, _iqrx) = futures::unsync::mpsc::channel(16);
    let (presencetx, _presencerx) = futures::unsync::mpsc::channel(16);
    let (_controltx, controlrx) = futures::unsync::mpsc::channel(16);
    let (psink, pstream) = Pipes::new(iqtx, dm, presencetx, controlrx).split();

    rt.spawn(
        pstream
        .forward(ReturnPath::new(tx.clone()))
        .map(|_| {})
    );

    let wait_for_stream_end = false;
    rt.block_on(
        stream.filter_map(move |event| {
            if wait_for_stream_end {
                None
            } else if event.is_online() {
                println!("Online");

                let presence = make_presence();
                tx.start_send(Packet::Stanza(presence)).unwrap();
                None
            } else {
                event.into_stanza()
            }
        })
        .map_err(|_| {})
        .forward(psink)
        .map(|_| {})
    ).unwrap();
}

pub struct ReturnPath {
    tx: mpsc::Sender<Packet>
}
impl ReturnPath {
    pub fn new(tx: mpsc::Sender<Packet>) -> Self {
        Self {
            tx
        }
    }
}
impl Sink for ReturnPath {
    type SinkItem = Element;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.tx.start_send(Packet::Stanza(item))
        .map(|x| x.map(|p| match p {
            Packet::Stanza(i) => i,
            _ => panic!("Sink returned different item on NotReady!"),
        }))
        .map_err(|_| {})
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.tx.poll_complete().map_err(|_| {})
    }
}

fn make_presence() -> Element {
    let mut presence = Presence::new(PresenceType::None);
    presence.show = PresenceShow::Chat;
    presence.statuses
        .insert(String::from("en"), String::from("Echoing messages."));
    presence.into()
}
