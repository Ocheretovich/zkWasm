//#![deny(unused_imports)]
//#![deny(dead_code)]
#![deny(unused_variables)]
#![allow(unused_imports)]
#![allow(dead_code)]
#![feature(thread_local)]

pub mod circuits;
pub mod cli;
pub mod foreign;
pub mod runtime;
pub mod traits;

mod profile;

#[cfg(test)]
pub mod test;

#[macro_use]
extern crate lazy_static;
extern crate downcast_rs;

// fn main() {
//     println!("Hello, world!");
// }
