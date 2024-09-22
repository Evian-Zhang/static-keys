#![feature(asm_goto)]

use static_keys::{define_static_key_false, static_branch_unlikely};

define_static_key_false!(MY_STATIC_KEY);

#[inline(always)]
fn foo() {
    println!("Entering foo function");
    if static_branch_unlikely!(MY_STATIC_KEY) {
        println!("A branch");
    } else {
        println!("B branch");
    }
}

fn main() {
    static_keys::global_init();
    foo();
    unsafe {
        MY_STATIC_KEY.enable();
    }
    foo();
}
