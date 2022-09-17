
use std::fmt;
use std::cmp::Ordering;
use url::Url;
use tungstenite::{connect, Message};
use rtrb;

use super::book as book;

#[derive(Debug, Default, Clone)]
pub struct BitmexOrder {
    pub id: String,
    pub price: f64,
    pub size:  f64,
}
impl fmt::Display for BitmexOrder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(id: {}, price: {}, size: {})", self.id, self.price, self.size)
    }
}
impl PartialOrd for BitmexOrder {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.price.partial_cmp(&other.price)
    }
}
impl PartialEq for BitmexOrder {
    fn eq(&self, other: &Self) -> bool {
        self.price == other.price
    }
}
#[derive(Debug, Default)]
pub struct BitmexBook {
    pub bids : Vec<BitmexOrder>,
    pub asks : Vec<BitmexOrder>,
}

fn bitmex_lower_bound(container: &Vec<BitmexOrder>, price : f64) -> usize {
    for i in 0..container.len() {
        if price <= container[i].price {
            return i;
        }
    }
    container.len()
}
fn bitmex_find(container: &Vec<BitmexOrder>, id : &String) -> usize {
    for i in 0..container.len() {
        if *id == container[i].id {
            return i;
        }
    }
    container.len()
}

pub fn print(book: &BitmexBook) {
    println!("    Bids:");
    for i in 0..book.bids.len() {
        println!("{} {}", i, book.bids[i]);
    }
    println!("    Asks:");
    for i in 0..book.asks.len() {
        println!("{} {}", i, book.asks[i]);
    }
}

pub fn bitmex(producer: &mut rtrb::Producer<book::OrderUpdate>) {
    let (mut socket, _response) = connect(Url::parse("wss://ws.bitmex.com/realtime").unwrap()).expect("Can't connect");
    println!("rcvd: {}", socket.read_message().expect("Error reading message"));

    // subscribe
    let result_subscribe = socket.write_message(Message::Text(r#"{"op": "subscribe", "args": ["orderBookL2_25:XBTUSD"]}"#.into()));
    match result_subscribe {
        Ok(_v) => println!("sent"),
        Err(e) => println!("error sending: {e:?}"),
    }

    // read and parse result_subscribe
    let response_subsribe = socket.read_message().expect("Error reading message");
    let json_response_subsribe: serde_json::Value = serde_json::from_str(response_subsribe.to_text().unwrap()).expect("response should be proper JSON");
    let subscribe_status = json_response_subsribe.get("success").expect("response should have success");
    if subscribe_status != &serde_json::json!(true) {
        println!("Subscription to bitmex failed");
        return;
    }

    let mut bitmex_book = BitmexBook{bids: Vec::new(), asks: Vec::new()};

    // read and parse snapshot
    {
        let snapshot_message = socket.read_message().expect("Error reading message");
        let json_snapshot: serde_json::Value = serde_json::from_str(snapshot_message.to_text().unwrap()).expect("snapshot should be proper JSON");
        //println!("snapshot: {}", json_snapshot);
        let snapshot_data = json_snapshot.get("data").expect("snapshot should have data").as_array().unwrap();
        for i in 0..snapshot_data.len() {
            //println!("bitmex snapshot order: {}", snapshot_data[i]);
            let order = BitmexOrder{id   : snapshot_data[i].get("id"   ).expect("order should have id   ").as_i64().unwrap().to_string()
                                   ,price: snapshot_data[i].get("price").expect("order should have price").as_f64().unwrap()
                                   ,size : snapshot_data[i].get("size" ).expect("order should have size ").as_f64().unwrap() / 1000000.0 // lot size for BTCUSD on bitmex is 100 satoshis https://www.bitmex.com/app/contract/XBTUSD
                                   };
            let is_sell = snapshot_data[i].get("side").expect("order should have side").as_str().unwrap() == "Sell";
            if is_sell {
                let idx = bitmex_lower_bound(&bitmex_book.asks, order.price);
                bitmex_book.asks.insert(idx, order.clone());
            } else {
                let idx = bitmex_lower_bound(&bitmex_book.bids, order.price);
                bitmex_book.bids.insert(idx, order.clone());
            }
            //print(&bitmex_book);

            let order_update = book::OrderUpdate {
                is_sell        : is_sell,
                price          : order.price,
                size_difference: order.size,
            };
            producer.push(order_update).expect("Error pushing order to queue");
        }
    }

    // read and parse updates
    loop {
        let msg = socket.read_message().expect("Error reading message");
        //println!("rcvd {}", msg);
        let json: serde_json::Value = serde_json::from_str(msg.to_text().unwrap()).expect("response should be proper JSON");
        let action = match json.get("action").expect("update should have action").as_str().unwrap() {
            "insert" => 0,
            "delete" => 1,
            "update" => 2,
            &_ => 3,
        };
        if action == 3 {
            println!("Action not processed: {}", json.get("action").expect("update should have action").as_str().unwrap());
            continue;
        }
        let data = json.get("data").expect("update should have data").as_array().unwrap();
        for i in 0..data.len()
        {
            //println!("bitmex order: {}", data[i]);
            let id = data[i].get("id").expect("order should have id").as_i64().unwrap().to_string();
            let is_sell = data[i].get("side").expect("order should have side").as_str().unwrap() == "Sell";
            let mut price = match data[i].get("price"){Some(v)=>v.as_f64().unwrap(),None=>0.0};
            let size  = match data[i].get("size" ){Some(v)=>v.as_f64().unwrap(),None=>0.0} / 1000000.0; // lot size for BTCUSD on bitmex is 100 satoshis https://www.bitmex.com/app/contract/XBTUSD
            let mut size_difference : f64 = 0.0;
            if action == 0 {
                //println!("insert");
                let order = BitmexOrder{id   : data[i].get("id").expect("order should have id").as_i64().unwrap().to_string()
                                       ,price: price
                                       ,size : size
                                       };
                if is_sell {
                    let idx = bitmex_lower_bound(&bitmex_book.asks, order.price);
                    bitmex_book.asks.insert(idx, order);
                    if bitmex_book.asks.len() > 25 {
                        let order = book::OrderUpdate {
                            is_sell         : is_sell,
                            price           : bitmex_book.asks[25].price,
                            size_difference : -bitmex_book.asks[25].size,
                        };
                        producer.push(order).expect("Error pushing order to queue");
                    }
                } else {
                    let idx = bitmex_lower_bound(&bitmex_book.bids, order.price);
                    bitmex_book.bids.insert(idx, order);
                    if bitmex_book.bids.len() > 25 {
                        let order = book::OrderUpdate {
                            is_sell         : is_sell,
                            price           : bitmex_book.bids[25].price,
                            size_difference : -bitmex_book.bids[25].size,
                        };
                        producer.push(order).expect("Error pushing order to queue");
                    }
                }
                size_difference = size;
            } else {
                if is_sell {
                    //println!("del/update sell");
                    let idx = bitmex_find(&bitmex_book.asks, &id);
                    if idx == bitmex_book.asks.len() {
                        println!("bitmex error: can't find order");
                        continue
                    }
                    price = bitmex_book.asks[idx].price;
                    if action == 1 {
                        // delete
                        //println!("del sell");
                        size_difference = -bitmex_book.asks[idx].size;
                        bitmex_book.asks.remove(idx);
                    } else {
                        // update
                        //println!("update sell");
                        //println!("old size: {}", bitmex_book.asks[idx].size);
                        //println!("update size: {}", size_difference);
                        size_difference = size - bitmex_book.asks[idx].size;
                        //println!("size difference: {}", size_difference);
                        bitmex_book.asks[idx].size = size;
                        //println!("new size: {}", bitmex_book.asks[idx].size);
                    }
                } else {
                    //println!("del/update buy");
                    let idx = bitmex_find(&bitmex_book.bids, &id);
                    if idx == bitmex_book.bids.len() {
                        println!("bitmex error: can't find order");
                        continue
                    }
                    price = bitmex_book.bids[idx].price;
                    if action == 1 {
                        // delete
                        size_difference = -bitmex_book.bids[idx].size;
                        bitmex_book.bids.remove(idx);
                    } else {
                        // update
                        size_difference = size - bitmex_book.bids[idx].size;
                        bitmex_book.bids[idx].size = size;
                    }
                }
            }
            //print(&bitmex_book);

            let order = book::OrderUpdate {
                is_sell         : is_sell,
                price           : price,
                size_difference : size_difference,
            };
            producer.push(order).expect("Error pushing order to queue");
        }
    }
}