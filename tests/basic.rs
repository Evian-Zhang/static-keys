#![feature(asm_goto)]
#![feature(asm_const)]

use std::sync::Once;

use static_keys::{
    define_static_key_false, define_static_key_true, static_branch_likely, static_branch_unlikely,
};

define_static_key_true!(INITIAL_TRUE_STATIC_KEY);
define_static_key_false!(INITIAL_FALSE_STATIC_KEY);

fn test_init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        static_keys::global_init();
    });
}

fn true_likely() -> usize {
    if static_branch_likely!(INITIAL_TRUE_STATIC_KEY) {
        1
    } else {
        2
    }
}

fn true_unlikely() -> usize {
    if static_branch_unlikely!(INITIAL_TRUE_STATIC_KEY) {
        1
    } else {
        2
    }
}

fn false_likely() -> usize {
    if static_branch_likely!(INITIAL_FALSE_STATIC_KEY) {
        1
    } else {
        2
    }
}

fn false_unlikely() -> usize {
    if static_branch_unlikely!(INITIAL_FALSE_STATIC_KEY) {
        1
    } else {
        2
    }
}

#[test]
fn test_true_key() {
    test_init();

    assert_eq!(true_likely(), 1);
    assert_eq!(true_unlikely(), 1);
    unsafe {
        INITIAL_TRUE_STATIC_KEY.disable();
    }
    assert_eq!(true_likely(), 2);
    assert_eq!(true_unlikely(), 2);
    unsafe {
        INITIAL_TRUE_STATIC_KEY.enable();
    }
    assert_eq!(true_likely(), 1);
    assert_eq!(true_unlikely(), 1);
}

#[test]
fn test_false_key() {
    test_init();

    assert_eq!(false_likely(), 2);
    assert_eq!(false_unlikely(), 2);
    unsafe {
        INITIAL_FALSE_STATIC_KEY.enable();
    }
    assert_eq!(false_likely(), 1);
    assert_eq!(false_unlikely(), 1);
    unsafe {
        INITIAL_FALSE_STATIC_KEY.disable();
    }
    assert_eq!(false_likely(), 2);
    assert_eq!(false_unlikely(), 2);
}
