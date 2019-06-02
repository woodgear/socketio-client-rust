use failure::{format_err, Error};
use serde_json;

#[derive(Debug, PartialEq)]
pub enum SocketIoEvent {
    Connect,
    Event(CustomStringEvent),
    DisConnect,
    Ping,
    Pong,
    TimeOut,
    ReConnect,
    Close,
    Open,
}

#[derive(Debug, PartialEq)]
pub struct CustomStringEvent {
    name: String,
    context: String,
}

impl CustomStringEvent {
    pub fn new<T1: ToString, T2: ToString>(name: T1, context: T2) -> Self {
        Self {
            name: name.to_string(),
            context: context.to_string(),
        }
    }

    pub fn encode(&self) -> String {
        return format!(r#"2["{}","{}"]"#, self.name, self.context);
    }
}

impl SocketIoEvent {
    pub fn encode(&self) -> String {
        match self {
            SocketIoEvent::Event(event) => {
                return format!("4{}", event.encode());
            }
            SocketIoEvent::Pong => {
                return "3".to_string();
            }

            SocketIoEvent::Ping => {
                return "2".to_string();
            }
            _ => {
                panic!("encode not impl yet");
            }
        }
    }

    pub fn parse<T: ToString>(data: T) -> Result<Self, Error> {
        let data = data.to_string();
        let data = data.as_str();
        return match data {
            "40" => Ok(SocketIoEvent::Open),
            "2" => Ok(SocketIoEvent::Ping),
            "3" => Ok(SocketIoEvent::Pong),
            data if data.starts_with("42") => {
                let res: [String; 2] = serde_json::from_str(&data[2..])?;
                return Ok(SocketIoEvent::Event(CustomStringEvent {
                    name: res[0].to_owned(),
                    context: res[1].to_owned(),
                }));
            }
            _ => Err(format_err!("not impl yet")),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cases(cases: Vec<(&str, SocketIoEvent)>) {
        for (raw, expect) in cases {
            test_case(raw, expect)
        }
    }

    fn test_case(raw: &str, expect: SocketIoEvent) {
        let res = SocketIoEvent::parse(raw).unwrap();
        assert_eq!(res, expect);
    }

    #[test]
    fn test_parser_event() {
        test_cases(vec![
            ("3", SocketIoEvent::Pong),
            ("2", SocketIoEvent::Ping),
            ("40", SocketIoEvent::Open),
            (
                "42[\"event\",\"test event\"]",
                SocketIoEvent::Event(CustomStringEvent {
                    name: "event".to_string(),
                    context: "test event".to_string(),
                }),
            ),
        ]);
    }
}
