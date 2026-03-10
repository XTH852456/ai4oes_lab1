#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::tangram::{render, Shape};

#[unsafe(no_mangle)]
extern "C" fn main() -> i32 {
    render(Shape::S, 2);
    0
}