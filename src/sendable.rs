use xmpp_parsers::{Jid, Element, TryFrom};
use xmpp_parsers::message::{Message, MessageType, Body, Subject, Thread};
use xmpp_parsers::delay::Delay;

use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct SendMessage {
    pub from: Option<Jid>,
    pub to: Option<Jid>,
    pub id: Option<String>,
    pub mtype: MessageType,
    pub body: Option<Body>,
    pub subject: Option<Subject>,
    pub thread: Option<Thread>,
    pub delay: Option<Delay>,
}
impl SendMessage {
    pub fn get_best<T>(map: &mut BTreeMap<String, T>, pref_langs: Vec<&str>) -> Option<T> {
        if map.is_empty() {
            return None;
        }

        for lang in pref_langs {
            if let Some(v) = map.remove(lang) {
                return Some(v);
            }
        }
        if let Some(v) = map.remove("") {
            return Some(v);
        }

        let k = map.keys().next().unwrap().clone();
        map.remove(&k)
    }

    pub fn get_best_body(m: &mut Message, pref_langs: Vec<&str>) -> Option<Body> {
        SendMessage::get_best::<Body>(&mut m.bodies, pref_langs)
    }
    pub fn get_best_subject(m: &mut Message, pref_langs: Vec<&str>) -> Option<Subject> {
        SendMessage::get_best::<Subject>(&mut m.subjects, pref_langs)
    }

    pub fn get_delay(m: &mut Message) -> Option<Delay> {
        while let Some(p) = m.payloads.pop() {
            if let Ok(d) = Delay::try_from(p) {
                return Some(d);
            }
        }
        return None;
    }
}
impl From<Message> for SendMessage {
    fn from(mut t: Message) -> SendMessage {
        let body = SendMessage::get_best_body(&mut t, vec!["en"]);
        let subject = SendMessage::get_best_subject(&mut t, vec!["en"]);
        let delay = SendMessage::get_delay(&mut t);

        SendMessage {
            from: t.from,
            to: t.to,
            id: t.id,
            mtype: t.type_,
            body, 
            subject,
            thread: t.thread,
            delay,
        }
    }
}
impl Into<Message> for SendMessage {
    fn into(self) -> Message {
        let mut bodies = BTreeMap::new();
        if let Some(b) = self.body { bodies.insert("en".into(), b); }
        let mut subjects = BTreeMap::new();
        if let Some(s) = self.subject { subjects.insert("en".into(), s); }
        let mut payloads = Vec::new();
        if let Some(d) = self.delay { payloads.push(d.into()); }

        Message {
            from: self.from,
            to: self.to,
            id: self.id,
            type_: self.mtype,
            bodies,
            subjects,
            thread: self.thread,
            payloads,
        }
    }
}
impl Into<Element> for SendMessage {
    fn into(self) -> Element {
        let m: Message = self.into();
        m.into()
    }
}
