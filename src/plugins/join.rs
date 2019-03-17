use futures::{future, Future};

use xmpp_parsers::{Jid, Element};
use xmpp_parsers::presence::{Presence, Show as PresenceShow, Type as PresenceType};

use crate::sendable::SendMessage;
use crate::command::make_reply;
use crate::command::FuturePlugin;

pub struct Join;

impl Join {
    pub fn new() -> Self {
        Join
    }
}
impl FuturePlugin for Join {
    fn call(&mut self, message: SendMessage) -> Box<Future<Item = Option<Element>, Error = ()>> {
        if let (Some(ref body), Some(ref from)) = (&message.body, &message.from) {
            if body.0.starts_with("^join") {
                if from.clone().into_bare_jid() == Jid::bare("dean4devil", "paranoidlabs.org") {
                    let mut chan = (body.0)[6..].split_whitespace();
                    if let Some(n) = chan.next() {
                        if let Some(d) = chan.next() {
                            let j = Jid::full(n,d,"gnutbot");
                            let mut presence = Presence::new(PresenceType::None);
                            presence.show = PresenceShow::None;
                            presence.to = Some(j);

                            return Box::new(future::ok(Some(presence.into())));
                        }
                    }
                }
            }
        }

        Box::new(future::ok(None))
    }
}
