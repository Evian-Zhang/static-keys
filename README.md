# static-keys

[Static key](https://docs.kernel.org/staging/static-keys.html) is a mechanism used by Linux kernel to speed up checks of seldomly used features. We brought it to Rust userland applications! (Currently nightly Rust required).

## Motivation

It's a common practice for modern applications to be configurable, either by CLI options or config files. Those values controlled by configuration flags are usually not changed after application initialization, and are frequently accessed during the whole application lifetime.

```rust, ignore
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
    jnz     do_something       ; If not zero, jump to do_somthing
do_common_routines:
    ; Do common routines
    ret
do_something:
    ; Do something
    jmp     do_common_routines ; Jump to do_common_routines
```

Although the `if`-check is just `test`-`jnz` instructions, it can still be speedup. What about making the check just a `jmp` (skip over the `do_something` branch) or `nop` (always `do_something`)? This is what static keys do. To put it simply, we **modify** the instruction at runtime. After getting the `flag` passed from commandline, we dynamically modify the `if flag {}` check to be a `jmp` or `nop` according to the `flag` value. For example, if user-specified `flag` is `true`, the assembled instructions will be **dynamically modified** to the following `nop` instruction.

```x86asm
    nop     DWORD PTR [rax+rax*1+0x0]
do_common_routines:
    ; Do common routines
    ret
do_something:
    ; Do something
    jmp     do_common_routines
```

There is no more `test` and conditional jumps, just a `nop` (which means this instruction does nothing).

Although replacing a `test`-`jnz` pair to `nop` may be minor improvement, however, as [documented in linux kernel](https://docs.kernel.org/staging/static-keys.html#motivation), if the configuration check involves global variables, this replacement can decrease memory cache pressure.

## Usage

To use this crate, currently nightly Rust is required. And in the crate root top, you should declare usage of unstable features `asm_goto` and `asm_const`.

Add this crate to your `Cargo.toml`:

```toml
[dependencies]
static-keys = "0.1"
```

Then at the beginning of `main` function, you should invoke [`static_keys::global_init`](https://docs.rs/static-keys/latest/static-keys/fn.global_init.html) to initialize.

```rust
#![feature(asm_goto)]
#![feature(asm_const)]

use static_keys::{
    define_static_key_false,
    static_branch_unlikely,
};

define_static_key_false!(FLAG_KEY);
```
