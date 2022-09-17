
use std::fmt;
//use std::cmp::Ordering;

#[derive(Debug, Default)]
pub struct Order {
    pub price: f64,
    pub size:  i64,
}
pub struct OrderUpdate  {
    pub is_sell: bool,
    pub price: f64,
    pub size_difference:i64,
}
impl fmt::Display for Order {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(price: {}, size: {})", self.price, self.size)
    }
}
/*
impl PartialOrd for Order {
    fn partial_cmp(&self, other: &OrderUpdate) -> Option<Ordering> {
        self.price.partial_cmp(&other.price)
    }
}
impl PartialEq for Order {
    fn eq(&self, other: &OrderUpdate) -> bool {
        self.price == other.price
    }
}
*/

pub fn book_lower_bound(container: &Vec<Order>, price : f64) -> usize {
    for i in 0..container.len() {
        if price <= container[i].price {
            return i;
        }
    }
    container.len()
}

#[derive(Debug, Default)]
pub struct Book {
    pub version : u64,
    pub bids : Vec<Order>,
    pub asks : Vec<Order>,
}
impl Book {
    pub fn new() -> Book {
        Book{version: 0, bids: Vec::new(), asks: Vec::new()}
    }
    pub fn get_spread(&self) -> f64 {
        //println!("{} bids, {} asks", self.get_bids_count(), self.get_asks_count());
        if self.bids.len() == 0 || self.asks.len() == 0 {
            return 0.0;
        }
        self.asks[0].price - self.bids[self.bids.len()-1].price
    }
    pub fn get_bids_count(&self)->usize {
        self.bids.len()
    }
    pub fn get_asks_count(&self)->usize {
        self.asks.len()
    }
    pub fn process(&mut self, order_update: &mut OrderUpdate) {
        //println!("order: {}", order);
        if order_update.is_sell {
            let idx = book_lower_bound(&self.asks, order_update.price);
            if idx >= self.asks.len() || self.asks[idx].price != order_update.price {
                self.asks.insert(idx, Order{price: order_update.price, size: order_update.size_difference});
            } else {
                self.asks[idx].size += order_update.size_difference;
                if self.asks[idx].size == 0 {
                    self.asks.remove(idx);
                }
            }
        } else {
            let idx = book_lower_bound(&self.bids, order_update.price);
            if idx >= self.bids.len() || self.bids[idx].price != order_update.price {
                self.bids.insert(idx, Order{price: order_update.price, size: order_update.size_difference});
            } else {
                self.bids[idx].size += order_update.size_difference;
                if self.bids[idx].size == 0 {
                    self.bids.remove(idx);
                }
            }
        }
        self.version += 1; 
        //self.print();
    }
    pub fn print(& self) {
        println!("    Bids:");
        for i in 0..self.bids.len() {
            println!("{} {}", i, self.bids[i]);
        }
        println!("    Asks:");
        for i in 0..self.asks.len() {
            println!("{} {}", i, self.asks[i]);
        }
    }
}
