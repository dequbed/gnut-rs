use futures::{StartSend, Poll, Async, Sink, AsyncSink, Future, Stream};
use futures::unsync::mpsc;
use xmpp_parsers::{Jid, Element, TryFrom};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::message::{Message, MessageType};
use xmpp_parsers::presence::Presence;

use std::collections::HashMap;

use crate::sendable::SendMessage;

pub struct Pipes {
    iq: mpsc::Sender<Iq>,
    message: ChatPipe,
    presence: mpsc::Sender<Presence>,

    command_router: futures::stream::Forward<mpsc::Receiver<PipeCmd>, CommandRouter>,
}
impl Pipes {
    pub fn new(iq: mpsc::Sender<Iq>, dm: ChannelHandler, presence: mpsc::Sender<Presence>, control: mpsc::Receiver<PipeCmd>) -> Self {
        let (message_cmd, message_rx) = mpsc::channel(16);

        let router = CommandRouter::new(message_cmd);
        Self {
            iq,
            message: ChatPipe::new(dm, message_rx),
            presence,

            command_router: control.forward(router),
        }
    }
}

impl Sink for Pipes {
    type SinkItem = Element;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match item.name() {
            "message" => if let Ok(m) = Message::try_from(item) {
                match self.message.start_send(m) {
                    Ok(AsyncSink::Ready) => return Ok(AsyncSink::Ready),
                    Ok(AsyncSink::NotReady(m)) => return Ok(AsyncSink::NotReady(m.into())),
                    Err(_) => return Err(()),
                }
            },
            "presence" => if let Ok(m) = Presence::try_from(item) {
                match self.presence.start_send(m) {
                    Ok(AsyncSink::Ready) => return Ok(AsyncSink::Ready),
                    Ok(AsyncSink::NotReady(m)) => return Ok(AsyncSink::NotReady(m.into())),
                    Err(_) => return Err(()),
                }
            },
            "iq" => if let Ok(m) = Iq::try_from(item) {
                match self.iq.start_send(m) {
                    Ok(AsyncSink::Ready) => return Ok(AsyncSink::Ready),
                    Ok(AsyncSink::NotReady(m)) => return Ok(AsyncSink::NotReady(m.into())),
                    Err(_) => return Err(()),
                }
            },
            _ => {}
        }
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        try_ready!(self.message.poll_complete().map_err(|_| ()));
        try_ready!(self.presence.poll_complete().map_err(|_| ()));
        try_ready!(self.iq.poll_complete().map_err(|_| ()));

        Ok(Async::Ready(()))
    }
}

impl Stream for Pipes {
    type Item = Element;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        Stream::poll(&mut self.message)
    }
}

impl Future for Pipes {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        Ok(Async::NotReady)
    }
}

struct CommandRouter {
    chat: mpsc::Sender<ChatCmd>,
}
impl CommandRouter {
    pub fn new(chat: mpsc::Sender<ChatCmd>) -> Self {
        Self {
            chat
        }
    }
}

impl Sink for CommandRouter {
    type SinkItem = PipeCmd;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match item {
            PipeCmd::MUC(c) => match self.chat.start_send(c) {
                Ok(AsyncSink::Ready) => return Ok(AsyncSink::Ready),
                Ok(AsyncSink::NotReady(c)) => return Ok(AsyncSink::NotReady(PipeCmd::MUC(c))),
                Err(_) => return Err(()),
            }
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        try_ready!(self.chat.poll_complete().map_err(|_| ()));

        Ok(Async::Ready(()))
    }
}

pub enum PipeCmd {
    MUC(ChatCmd),
}

struct ChatPipe {
    dm: ChannelHandler,
    muc: HashMap<Jid, ChannelHandler>,

    control: mpsc::Receiver<ChatCmd>,
}
impl ChatPipe {
    pub fn new(dm: ChannelHandler, control: mpsc::Receiver<ChatCmd>) -> Self {
        Self {
            dm,
            muc: HashMap::new(),
            control,
        }
    }

    pub fn join(&mut self, j: Jid, c: ChannelHandler) -> Option<ChannelHandler> {
        self.muc.insert(j.into_bare_jid(), c)
    }

    pub fn leave(&mut self, j: Jid) -> Option<ChannelHandler> {
        self.muc.remove(&j.into_bare_jid())
    }
}

impl Sink for ChatPipe {
    type SinkItem = Message;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match item.type_ {
            MessageType::Chat => match self.dm.start_send(item.into()) {
                Ok(AsyncSink::Ready) => return Ok(AsyncSink::Ready),
                Ok(AsyncSink::NotReady(m)) => return Ok(AsyncSink::NotReady(m.into())),
                Err(_) => return Err(()),
            },
            MessageType::Groupchat => {
                println!("GC to {:?}", &item.to);
                if let Some(j) = item.to.clone() {
                    //if let Some(c) = self.muc.get_mut(&j.into_bare_jid()) {
                        match self.dm.start_send(item.into()) {
                            Ok(AsyncSink::Ready) => return Ok(AsyncSink::Ready),
                            Ok(AsyncSink::NotReady(m)) => return Ok(AsyncSink::NotReady(m.into())),
                            Err(_) => return Err(()),
                       // }
                    }
                }

                return Ok(AsyncSink::Ready);
            },
            _ => {
                println!("Received Unhandled Message type {:?} in {:?}", item.type_, item);
                return Ok(AsyncSink::Ready);
            }
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        try_ready!(self.dm.poll_complete().map_err(|_| ()));
        for v in self.muc.values_mut() {
            try_ready!(v.poll_complete().map_err(|_| ()));
        }

        Ok(Async::Ready(()))

    }
}
impl Stream for ChatPipe {
    type Item = Element;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.dm.poll()? {
            Async::Ready(Some(e)) => return Ok(Async::Ready(Some(e))),
            Async::Ready(None) => return Ok(Async::Ready(None)),
            Async::NotReady => {  }
        }

        for c in self.muc.values_mut() {
            match c.poll()? {
                Async::Ready(Some(e)) => return Ok(Async::Ready(Some(e))),
                Async::Ready(None) => return Ok(Async::Ready(None)),
                Async::NotReady => {  }
            }
        }

        Ok(Async::NotReady)
    }
}
impl Future for ChatPipe {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let cmd = try_ready!(self.control.poll().map_err(|_| {}));

        match cmd {
            Some(ChatCmd::Join(j, chan)) => {
                self.join(j, chan);
            },
            Some(ChatCmd::Leave(j)) => {
                self.leave(j);
            },
            None => return Ok(Async::Ready(())),
        }

        Ok(Async::NotReady)
    }
}

enum ChatCmd {
    Join(Jid, ChannelHandler),
    Leave(Jid),
}

pub struct ChannelHandler {
    dispatch: Vec<Dispatch>,
    select: Vec<Select>
}
impl ChannelHandler {
    pub fn new() -> Self {
        Self::new_with_modules(Vec::new(), Vec::new())
    }

    pub fn new_with_modules(sinks: Vec<Box<Sink<SinkItem = SendMessage, SinkError = ()>>>,
                            streams: Vec<Box<Stream<Item = Element, Error = ()>>>
        ) -> Self {
        let sii = sinks.into_iter();
        let sti = streams.into_iter();
        Self {
            dispatch: sii.map(|x| Dispatch::new(x)).collect(),
            select: sti.map(|x| Select::new(x)).collect(),
        }
    }

    pub fn add_sink(&mut self, sink: Box<Sink<SinkItem = SendMessage, SinkError = ()>>) {
        self.dispatch.push(Dispatch::new(sink));
    }

    pub fn add_stream(&mut self, stream: Box<Stream<Item = Element, Error = ()>>) {
        self.select.push(Select::new(stream));
    }
}
impl Sink for ChannelHandler {
    type SinkItem = SendMessage;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let mut ready = true;
        for d in self.dispatch.iter_mut() {
            ready = ready | d.is_ready()?;
        }
        if ready {
            for d in self.dispatch.iter_mut() {
                d.start_send(item.clone()).expect("Dispatch signaled NotReady after flushing completely!");
            }

            Ok(AsyncSink::Ready)
        } else {
            Ok(AsyncSink::NotReady(item))
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        for d in self.dispatch.iter_mut() {
            try_ready!(d.poll_complete());
        }
        Ok(Async::Ready(()))
    }
}
impl Stream for ChannelHandler {
    type Item = Element;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        for s in self.select.iter_mut() {
            match s.poll()? {
                Async::Ready(Some(v)) => return Ok(Async::Ready(Some(v))),
                Async::Ready(None) => return Ok(Async::Ready(None)),
                Async::NotReady => {}
            }
        }
        return Ok(Async::NotReady)
    }
}

struct Dispatch {
    inner: Box<Sink<SinkItem=SendMessage, SinkError=()>>,
    state: AsyncSink<()>,
}
impl Dispatch {
    pub fn new(inner: Box<Sink<SinkItem=SendMessage, SinkError=()>>) -> Self{
        Self {
            inner,
            state: AsyncSink::NotReady(())
        }
    }

    pub fn is_ready(&mut self) -> Result<bool, <Self as Sink>::SinkError> {
        match self.poll_complete()? {
            Async::Ready(()) => Ok(true),
            Async::NotReady => Ok(false)
        }
    }
}
impl Sink for Dispatch {
    type SinkItem = SendMessage;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match self.state {
            AsyncSink::NotReady(_) => {
                return Ok(AsyncSink::NotReady(item));
            },
            AsyncSink::Ready => {
                self.inner.start_send(item)
            }
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        try_ready!(self.inner.poll_complete());
        self.state = AsyncSink::Ready;
        Ok(Async::Ready(()))
    }
}

struct Select {
    stream: Box<Stream<Item=Element, Error=()>>,
}
impl Select {
    pub fn new(stream: Box<Stream<Item=Element, Error=()>>) -> Self {
        Self {
            stream,
        }
    }
}
impl Stream for Select {
    type Item = Element;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.stream.poll()
    }
}
