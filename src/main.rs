
use std::sync::Arc;
use std::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use rtrb::{RingBuffer};
use tokio::sync::mpsc;


pub mod book;
pub mod bitmex;
//pub mod bitfinex;
pub mod kraken;

use bookreq::booker_server::{Booker, BookerServer};
use bookreq::{BookResponse, BookRequest};

// Import the generated proto-rust file into a module
pub mod bookreq {
    tonic::include_proto!("bookreq");
}
#[derive(Debug, Default)]
pub struct MyBooker {
    book : Arc<Mutex<book::Book>>
}

fn make_response(version: &mut u64, book : Arc<Mutex<book::Book>>) -> Option<bookreq::BookResponse> {
    let b = book.lock().unwrap();
    if b.version == *version {
        return None;
    }
    *version = b.version;
    let mut bids : Vec<bookreq::Order> = Vec::new();
    let mut asks : Vec<bookreq::Order> = Vec::new();
    for i in 0..b.bids.len() {
        if i == 10 {
            break;
        }
        bids.push(bookreq::Order{price: b.bids[b.bids.len()-1-i].price, size: b.bids[i].size});
    }
    for i in 0..b.asks.len() {
        if i == 10 {
            break;
        }
        asks.push(bookreq::Order{price: b.asks[i].price, size: b.asks[i].size});
    }
    Some(bookreq::BookResponse {
        spread: b.get_spread(),
        bids: bids,
        asks: asks,
    })
}

// Implement the service function(s) defined in the proto
// for the Booker service (Get...)
#[tonic::async_trait]
impl Booker for MyBooker {
    async fn get(&self, _request: Request<BookRequest>) -> Result<Response<BookResponse>, Status> {
        let mut version : u64 = 0;
        let response = make_response(&mut version, self.book.clone());
        //bookreq::BookResponse {
        //    spread: self.book.lock().unwrap().get_spread(),
        //};
        match response {
            Some(r) => Ok(Response::new(r)),
            None => Err(Status::invalid_argument("no book yet")),
        }
        //Ok(Response::new(response))
    }
    type GettingStream=mpsc::Receiver<Result<BookResponse,Status>>;
    async fn getting(&self, _request: Request<BookRequest>) -> Result<Response<Self::GettingStream>, Status> {
        // creating a queue or channel
        let (mut tx, rx) = mpsc::channel(4);
        let book = self.book.clone();
        let mut version = 0;
        // creating a new task
        tokio::spawn(async move {
            loop {
                let response = make_response(&mut version, book.clone());
                if response != None {
                    tx.send(Ok(response.unwrap())).await;
                }
            }
        });
        // returning our reciever so that tonic can listen on reciever and send the response to client
        Ok(Response::new(rx))
    }
}

fn aggregate(book: Arc<Mutex<book::Book>>) {
    let (mut producer_bitmex, mut consumer_bitmex) = RingBuffer::<book::OrderUpdate>::new(1024*1024);
    let (mut producer_kraken, mut consumer_kraken) = RingBuffer::<book::OrderUpdate>::new(1024*1024);
    std::thread::spawn(move || { bitmex::bitmex(&mut producer_bitmex); });
    std::thread::spawn(move || { kraken::kraken(&mut producer_kraken); });

    loop {
        if !consumer_bitmex.is_empty() {
            let mut locked_book = book.lock().unwrap();
            while !consumer_bitmex.is_empty() {
                let mut order = consumer_bitmex.pop().expect("Error popping from bitmex queue");
                locked_book.process(&mut order);
            }
            locked_book.version += 1;
        }
        if !consumer_kraken.is_empty() {
            let mut locked_book = book.lock().unwrap();
            while !consumer_kraken.is_empty() {
                let mut order = consumer_kraken.pop().expect("Error popping from kraken queue");
                locked_book.process(&mut order);
            }
            locked_book.version += 1;
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let book : Arc<Mutex<book::Book>> = Arc::new(Mutex::new(book::Book::new()));

    let addr = "127.0.0.1:50051".parse()?;
    let booker = MyBooker{book: book.clone()};

    std::thread::spawn(move || { aggregate(book.clone()); });

    println!("Starting gRPC Server...");
    Server::builder()
        .add_service(BookerServer::new(booker))
        .serve(addr)
        .await?;

    Ok(())
}

#[test]
fn kraken_parse_size() {
    assert_eq!(kraken::parse_size_into_satoshis( r#""0.00000000""#),   00000000i64);
    assert_eq!(kraken::parse_size_into_satoshis( r#""0.10000000""#),   10000000i64);
    assert_eq!(kraken::parse_size_into_satoshis( r#""1.00000000""#),  100000000i64);
    assert_eq!(kraken::parse_size_into_satoshis( r#""9.90000000""#),  990000000i64);
    assert_eq!(kraken::parse_size_into_satoshis(r#""10.00000001""#), 1000000001i64);
}