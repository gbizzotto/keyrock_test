
use url::Url;
use tungstenite::{connect, Message};
use rtrb;

use super::book as book;

pub fn kraken(producer: &mut rtrb::Producer<book::OrderUpdate>) {
    let (mut socket, _response) = connect(Url::parse("wss://ws.kraken.com").unwrap()).expect("Can't connect");
    println!("rcvd: {}", socket.read_message().expect("Error reading message"));

    // subscribe
    let result_subscribe = socket.write_message(Message::Text(r#"{"event":"subscribe", "subscription":{"name":"book", "depth": 25}, "pair":["BTC/USD"]}"#.into()));
    match result_subscribe {
        Ok(_v) => println!("sent"),
        Err(e) => println!("error sending: {e:?}"),
    }

    let mut kraken_book = book::Book::new();

    // read and parse result_subscribe
    let response_subsribe = socket.read_message().expect("Error reading message");
    let json_response_subsribe: serde_json::Value = serde_json::from_str(response_subsribe.to_text().unwrap()).expect("response should be proper JSON");
    println!("subs: {}", json_response_subsribe);
    let subscribe_status = json_response_subsribe.get("status").expect("response should have success");
    if subscribe_status != "subscribed" {
        println!("Subscription to kraken failed");
        return;
    }
    // read and parse snapshot
    loop {
        let snapshot_message = socket.read_message().expect("Error reading message");
        let json_snapshot: serde_json::Value = serde_json::from_str(snapshot_message.to_text().unwrap()).expect("snapshot should be proper JSON");
        //println!("snapshot: {}", json_snapshot);
        if json_snapshot.get("event") == None {
            let bids = json_snapshot[1]["bs"].as_array().unwrap();
            let asks = json_snapshot[1]["as"].as_array().unwrap();

            for i in 0..bids.len() {
                let bid = bids[i].as_array().unwrap();
                let order = book::Order {
                    price: bid[0].to_string().replace("\"", "").parse::<f64>().unwrap(),
                    size : bid[1].to_string().replace("\"", "").parse::<f64>().unwrap() * 100000000.0, // size in satoshis
                };
                let idx = book::book_lower_bound(&kraken_book.bids, order.price);
                kraken_book.bids.insert(idx, book::Order{price: order.price, size: order.size});
                let order_update = book::OrderUpdate {
                    is_sell        : false,
                    price          : order.price,
                    size_difference: order.size,
                };
                producer.push(order_update).expect("Error pushing order to queue");
            }
            for i in 0..asks.len() {
                let ask = asks[i].as_array().unwrap();
                let order = book::Order {
                    price: ask[0].to_string().replace("\"", "").parse::<f64>().unwrap(),
                    size : ask[1].to_string().replace("\"", "").parse::<f64>().unwrap() * 100000000.0, // size in satoshis
                };
                let idx = book::book_lower_bound(&kraken_book.asks, order.price);
                kraken_book.asks.insert(idx, book::Order{price: order.price, size: order.size});
                let order_update = book::OrderUpdate {
                    is_sell        : true,
                    price          : order.price,
                    size_difference: order.size,
                };
                producer.push(order_update).expect("Error pushing order to queue");
            }
            break;
        }
    }

    // read and parse updates
    loop {
        let msg = socket.read_message().expect("Error reading message");
        //println!("rcvd {}", msg);
        let json: serde_json::Value = serde_json::from_str(msg.to_text().unwrap()).expect("response should be proper JSON");
        //println!("update: {}", json);

        if json[1]["b"].as_array() != None {
            let bids = json[1]["b"].as_array().unwrap();
            for i in 0..bids.len() {
                let bid = bids[i].as_array().unwrap();
                let order = book::Order {
                    price: bid[0].to_string().replace("\"", "").parse::<f64>().unwrap(),
                    size : bid[1].to_string().replace("\"", "").parse::<f64>().unwrap(),
                };
                let idx = book::book_lower_bound(&kraken_book.bids, order.price);
                if idx >= kraken_book.bids.len() || kraken_book.bids[idx].price != order.price {
                    // insert
                    kraken_book.bids.insert(idx, book::Order{price: order.price, size: order.size});
                    if kraken_book.bids.len() > 25 {
                        let order = book::OrderUpdate {
                            is_sell         : false,
                            price           : kraken_book.bids[25].price,
                            size_difference : -kraken_book.bids[25].size,
                        };
                        producer.push(order).expect("Error pushing order to queue");
                    }
                    let order_update = book::OrderUpdate {
                        is_sell        : false,
                        price          : order.price,
                        size_difference: order.size,
                    };
                    producer.push(order_update).expect("Error pushing order to queue");
                } else if order.size == 0.0 {
                    // delete
                    let order_update = book::OrderUpdate {
                        is_sell        : false,
                        price          : order.price,
                        size_difference: -order.size,
                    };
                    producer.push(order_update).expect("Error pushing order to queue");
                    kraken_book.bids.remove(idx);
                } else {
                    // update
                    let order_update = book::OrderUpdate {
                        is_sell        : false,
                        price          : order.price,
                        size_difference: order.size - kraken_book.bids[idx].size,
                    };
                    producer.push(order_update).expect("Error pushing order to queue");
                    kraken_book.bids[idx].size = order.size;
                }
            }
        }
        if json[1]["a"].as_array() != None {
            let asks = json[1]["a"].as_array().unwrap();
            for i in 0..asks.len() {
                let ask = asks[i].as_array().unwrap();
                let order = book::Order {
                    price: ask[0].to_string().replace("\"", "").parse::<f64>().unwrap(),
                    size : ask[1].to_string().replace("\"", "").parse::<f64>().unwrap(),
                };
                let idx = book::book_lower_bound(&kraken_book.asks, order.price);
                if idx >= kraken_book.asks.len() || kraken_book.asks[idx].price != order.price {
                    // insert
                    kraken_book.asks.insert(idx, book::Order{price: order.price, size: order.size});
                    if kraken_book.asks.len() > 25 {
                        let order = book::OrderUpdate {
                            is_sell         : true,
                            price           : kraken_book.asks[25].price,
                            size_difference : -kraken_book.asks[25].size,
                        };
                        producer.push(order).expect("Error pushing order to queue");
                    }
                    let order_update = book::OrderUpdate {
                        is_sell        : true,
                        price          : order.price,
                        size_difference: order.size,
                    };
                    producer.push(order_update).expect("Error pushing order to queue");
                } else if order.size == 0.0 {
                    // delete
                    let order_update = book::OrderUpdate {
                        is_sell        : true,
                        price          : order.price,
                        size_difference: -order.size,
                    };
                    producer.push(order_update).expect("Error pushing order to queue");
                    kraken_book.asks.remove(idx);
                } else {
                    // update
                    let order_update = book::OrderUpdate {
                        is_sell        : true,
                        price          : order.price,
                        size_difference: order.size - kraken_book.asks[idx].size,
                    };
                    producer.push(order_update).expect("Error pushing order to queue");
                    kraken_book.asks[idx].size = order.size;
                }
            }
        }
        //kraken_book.print();
    }
}