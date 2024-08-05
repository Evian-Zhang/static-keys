# static-keys

[![Rust CI status](https://github.com/Evian-Zhang/static-keys/actions/workflows/ci.yml/badge.svg)](https://github.com/Evian-Zhang/static-keys/actions/workflows/ci.yml)
[![Crates.io Version](https://img.shields.io/crates/v/static-keys)](https://crates.io/crates/static-keys)
[![docs.rs](https://img.shields.io/docsrs/static-keys?logo=docs.rs)](https://docs.rs/static-keys)

[Static key](https://docs.kernel.org/staging/static-keys.html) is a mechanism used by Linux kernel to speed up checks of seldomly changed features. We brought it to Rust userland applications for Linux, macOS and Windows! (Currently nightly Rust required. For reasons, see [FAQ](https://evian-zhang.github.io/static-keys/en/FAQs.html#why-is-nightly-rust-required)).

Currently CI-tested platforms:

* Linux

    * `x86_64-unknown-linux-gnu`
    * `x86_64-unknown-linux-musl`
    * `i686-unknown-linux-gnu`
    * `aarch64-unknown-linux-gnu`
* macOS

    * `aarch64-apple-darwin`
* Windows

    * `x86_64-pc-windows-msvc`
    * `i686-pc-windows-msvc`

For more comprehensive explanations and FAQs, you can refer to [GitHub Pages](https://evian-zhang.github.io/static-keys/en/)([中文版文档](https://evian-zhang.github.io/static-keys/zh-Hans/)).

## Motivation

It's a common practice for modern applications to be configurable, either by CLI options or config files. Those values controlled by configuration flags are usually not changed after application initialization, and are frequently accessed during the whole application lifetime.

```rust,ignore
let flag = CommandlineArgs::parse();
loop {
    if flag {
        do_something();
    }
    do_common_routines();
}
```

Although `flag` will not be modified after application initialization, the `if`-check still happens in each round, and in x86-64, it may be compiled to the following `test`-`jnz` instructions.

```x86asm
    test    eax, eax           ; Check whether eax register is 0
    jnz     do_something       ; If not zero, jump to do_something
do_common_routines:
    ; Do common routines
    ret
do_something:
    ; Do something
    jmp     do_common_routines ; Jump to do_common_routines
```

Although the `if`-check is just `test`-`jnz` instructions, it can still be speedup. What about making the check just a `jmp` (skip over the `do_something` branch) or `nop` (always `do_something`)? This is what static keys do. To put it simply, we **modify** the instruction at runtime. After getting the `flag` passed from commandline, we dynamically modify the `if flag {}` check to be a `jmp` or `nop` according to the `flag` value.

For example, if user-specified `flag` is `false`, the assembled instructions will be **dynamically modified** to the following `nop` instruction.

```x86asm
    nop     DWORD PTR [rax+rax*1+0x0]
do_common_routines:
    ; Do common routines
    ret
do_something:
    ; Do something
    jmp     do_common_routines
```

If `flag` is `true`, then we will dynamically modify the instruction to an unconditional jump instruction:

```x86asm
    jmp     do_something
do_common_routines:
    ; Do common routines
    ret
do_something:
    ; Do something
    jmp     do_common_routines
```

There is no more `test` and conditional jumps, just a `nop` (which means this instruction does nothing) or `jmp`.

Although replacing a `test`-`jnz` pair to `nop` may be minor improvement, however, as [documented in linux kernel](https://docs.kernel.org/staging/static-keys.html#motivation), if the configuration check involves global variables, this replacement can decrease memory cache pressure. And in server applications, such configuration may be shared between multiple threads in `Arc`s, which has much more overhead than just `nop` or `jmp`.

## Usage

To use this crate, currently nightly Rust is required. And in the crate root top, you should declare usage of unstable features `asm_goto` and `asm_const`.

```rust
#![feature(asm_goto)]
#![feature(asm_const)]
```

First, add this crate to your `Cargo.toml`:

```toml
[dependencies]
static-keys = "0.3"
```

At the beginning of `main` function, you should invoke [`static_keys::global_init`](https://docs.rs/static-keys/latest/static_keys/fn.global_init.html) to initialize.

```rust
fn main() {
    static_keys::global_init();
    // Do other things...
}
```

Then you should define a static key to hold the value affected by user-controlled flag, and enable or disable it according to the user passed flag.

```rust
# use static_keys::define_static_key_false;
# struct CommandlineArgs {}
# impl CommandlineArgs { fn parse() -> bool { true } }
// FLAG_STATIC_KEY is defined with initial value `false`
define_static_key_false!(FLAG_STATIC_KEY);

fn application_initialize() {
    let flag = CommandlineArgs::parse();
    if flag {
        unsafe {
            FLAG_STATIC_KEY.enable();
        }
    }
}
```

Note that you can enable or disable the static key any number of times at any time. And more importantly, **it is very dangerous if you modify a static key in a multi-threads environment**. Always spawn threads after you complete the modification of such static keys. And to make it more clear, **it is absolutely safe to use this static key in multi-threads environment** as below. The modification of static keys may be less efficient, while since the static keys are used to seldomly changed features, the modifications rarely take place, so the inefficiency does not matter. See [FAQ](https://evian-zhang.github.io/static-keys/en/FAQs.html#why-static-keys-must-only-be-modified-in-a-single-thread-environment) for more explanation.

After the definition, you can use this static key at `if`-check as usual (you can see [here](https://doc.rust-lang.org/std/intrinsics/fn.likely.html) and [here](https://kernelnewbies.org/FAQ/LikelyUnlikely) to know more about the `likely`-`unlikely` API semantics). A static key can be used at multiple `if`-checks. If the static key is modified, all locations using this static key will be modified to `jmp` or `nop` accordingly.

```rust
# #![feature(asm_goto)]
# #![feature(asm_const)]
# use static_keys::{define_static_key_false, static_branch_unlikely};
# struct CommandlineArgs {}
# impl CommandlineArgs { fn parse() -> bool { true } }
# fn do_something() {}
# fn do_common_routines() {}
# define_static_key_false!(FLAG_STATIC_KEY);
fn run() {
    loop {
        if static_branch_unlikely!(FLAG_STATIC_KEY) {
            do_something();
        }
        do_common_routines();
    }
}
```

## References

* The Linux kernel official documentation : [static-keys](https://docs.kernel.org/staging/static-keys.html)
* [Linux `static_key` internals](https://terenceli.github.io/%E6%8A%80%E6%9C%AF/2019/07/20/linux-static-key-internals)
* Rust for Linux also has an implementation for static keys, please refer to [Rust-for-Linux/linux#1084](https://github.com/Rust-for-Linux/linux/pull/1084) for more information. My implementaion's `break`-based inline assembly layout is inspired by this great work.
