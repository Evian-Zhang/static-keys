# static-keys

[Static key](https://docs.kernel.org/staging/static-keys.html) is a mechanism used by Linux kernel to speed up checks of seldomly used features. We brought it to Rust userland applications! (Currently nightly Rust required).

## Motivation

It's a common practice for modern applications to be configurable, either by CLI options or config files. Those values controlled by configuration flags are usually not changed after application initialization, and are frequently accessed during the whole application lifetime.

```rust
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

If user-specified `flag` is `true`, then we will dynamically modify the instruction to an unconditional jump instruction:

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

Although replacing a `test`-`jnz` pair to `nop` may be minor improvement, however, as [documented in linux kernel](https://docs.kernel.org/staging/static-keys.html#motivation), if the configuration check involves global variables, this replacement can decrease memory cache pressure.

## Usage

To use this crate, currently nightly Rust is required. And in the crate root top, you should declare usage of unstable features `asm_goto` and `asm_const`.

```rust
#![feature(asm_goto)]
#![feature(asm_const)]
```

First, add this crate to your `Cargo.toml`:

```toml
[dependencies]
static-keys = "0.2"
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
// FLAG_STATIC_KEY is defined with initial value `false`
define_static_key_false!(FLAG_STATIC_KEY);

fn application_initialize() {
    let flag = CommandlineArgs::parse();
    if flag {
        unsafe{
            FLAG_STATIC_KEY.enable();
        }
    }
}
```

Note that you can enable or disable the static key any number of times at any time, it is very dangerous if you modify the static key at one thread, and use it at another thread. So always make sure you are modifying a static key which is not used at the same time.

After the definition, you can use this static key at `if`-check as usual (you can see [here](https://doc.rust-lang.org/std/intrinsics/fn.likely.html) to know more about the `likely`-`unlikely` API semantics).

```rust
fn run() {
    loop {
        if static_branch_unlikely!(FLAG_STATIC_KEY) {
            do_something();
        }
        do_common_routines();
    }
}
```
