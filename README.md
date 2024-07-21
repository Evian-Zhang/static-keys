# static-keys

[Static key](https://docs.kernel.org/staging/static-keys.html) is a mechanism used by Linux kernel to speed up checks of seldomly used features. We brought it to Rust userland applications!

## Example

A common example is about some flags user passed from commandline options to a long-running CLI applications. It is annoying to check a value that will never be changed after application boot up:

```rust, ignore
let flag = CommandlineArgs::parse();
loop {
    if flag {
        do_something();
    }
    do_common_routines();
}
```

Although the `if`-check is just `test`-`jnz` instructions in x86, it can still be speedup. What about making the check just as a `jmp` (skip over the `do_something` branch) or `nop` (always `do_something`)? This is what static keys do. To put it simply, we **modify** the instruction at runtime. After getting the `flag` passed from commandline, we dynamically modify the `if flag` instruction to be a `jmp` or `nop` according to the `flag` value.

## Usage

See [Example](./examples/usage.rs).
