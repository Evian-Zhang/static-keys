#![feature(asm_goto)]
#![feature(asm_const)]

use static_keys::{define_static_key_false, static_key_unlikely, StaticKey};

define_static_key_false!(MY_STATIC_KEY);

fn foo() {
    println!("Entering foo function");
    if static_key_unlikely!(MY_STATIC_KEY) {
        println!("A branch");
    } else {
        println!("B branch");
    }
}

fn main() {
    unsafe {
        StaticKey::global_init();
    }
    foo();
    unsafe {
        MY_STATIC_KEY.enable();
    }
    foo();
}
