use futures::{future, Future};

use crate::pipes::SendMessage;
use crate::command::make_reply;
use crate::command::FutureMessagePlugin;

use std::str::SplitWhitespace;

use random;
use random::Source;

pub struct CommandM {
    dice: Dice,
}

impl CommandM {
    pub fn new() -> Self {
        CommandM {
            dice: Dice::new(),
        }
    }
}
impl FutureMessagePlugin for CommandM {
    fn call(&mut self, message: SendMessage) -> Box<Future<Item = Option<SendMessage>, Error = ()>> {
        if let Some(ref body) = &message.body {
            if body.0.starts_with("^") {
                let mut i = body.0.split_whitespace();
                let c = i.next().map(|s| s.trim_start_matches("^"));
                let o = match c {
                    Some(x) if x == Dice::command() => Some(self.dice.call(&message, i)),
                    Some(_) => None,
                    None => None,
                };

                return Box::new(future::ok(o.map(|m| make_reply(&message, m))));
            }
        }

        Box::new(future::ok(None))
    }
}

trait Command {
    fn new() -> Self;
    fn command() -> &'static str;
    fn call(&mut self, m: &SendMessage, args: SplitWhitespace) -> String;
}

struct Dice {
    random: random::Default,
}

impl Command for Dice {
    fn new() -> Self {
        Dice {
            random: random::Default::new(),
        }
    }

    fn command() -> &'static str {
        "throw"
    }

    fn call(&mut self, m: &SendMessage, mut args: SplitWhitespace) -> String {
        if let Some(a) = args.next() {
            let v: Vec<&str> = a.split('d').collect();
            if v.len() == 2 {
                if let (Ok(x),Ok(y)) = (u64::from_str_radix(v[0], 10), u64::from_str_radix(v[1], 10)) {
                    if x < 256 {
                        let mut out = Vec::new();
                        for _ in (1..=x).rev() {
                             out.push((self.random.read_u64() % y) + 1);
                        }

                        if x == 1 {
                            return format!("{}", out[0]);
                        } else {
                            let sum: u64 = out.iter().sum();
                            return format!("{:?} | {}", out, sum);
                        }
                    } else {
                        return "How about no?".into();
                    }
                }
            }
        }

        "Usage: ^throw [X]d[Y], e.g. 6d20 to throw 6 20-sided dice".into()
    }
}
