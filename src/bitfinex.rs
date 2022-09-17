
use url::Url;
use tungstenite::{connect, Message};
use rtrb;

use super::book as book;

pub fn bitfinex(producer: &mut rtrb::Producer<book::OrderUpdate>) {
    let (mut socket, _response) = connect(Url::parse("wss://api-pub.bitfinex.com/ws/2").unwrap()).expect("Can't connect");
    println!("rcvd: {}", socket.read_message().expect("Error reading message"));

    let result_subscribe = socket.write_message(Message::Text(r#"{ 
      event: 'subscribe', 
      channel: 'book', 
      symbol: 'tBTCUSD'
    }"#.into()));
    match result_subscribe {
        Ok(_v) => println!("sent"),
        Err(e) => println!("error sending: {e:?}"),
    }

    loop {
        let msg = socket.read_message().expect("Error reading message");
        println!("rcvd: {}", msg);
    }
}
