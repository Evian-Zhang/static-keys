# FAQs

## Why static key should be applied to seldomly changed features?

Two reasons:

* The modification of static key will impose a potential security risk since we need to bypass DEP. The DEP will be restored after modification done.
* The modification is less efficient since many syscalls are involved.

## Why static keys must only be modified in a single-thread environment?

In userland, it is very complicated to modify an instruction which may be executed by another thread. Linux kernel community once proposed a [`text_poke` syscall](https://lwn.net/Articles/574309/), but is still not available nowadays. BTW, [Linus doesn't seem to like it](https://lore.kernel.org/lkml/CA+55aFzr9ZKcGfT_Q31T9_vuCcmWxGCh0wixuZqt7VhjxxYU9g@mail.gmail.com/), and his reasons do make sense.

Another reason is that we need to manipulate memory protection to bypass DEP, which may involves race condition on the protection itself in multi-thread environment. Mutex may be used to avoid data race, while if cargo resolves multi-version static-key crates dependencies, the mutexes would be duplicated for each version, and this approach is thus useless. This shall be resolved when [RFC 1977: public & private dependencies](https://github.com/rust-lang/rust/issues/44663) is stabilized. [rust-lang/cargo#2363](https://github.com/rust-lang/cargo/issues/2363) is also a reference.

## Why `static_branch_likely!` and `static_branch_unlikely!` are implemented as macros?

Because when passing a static variable to inline assembly as `sym` argument, it requires the argument to be a static path. We cannot construct such path in a function.

## What OS-specific features are required to extend to new OSs?

* Symbols indicating a custom section's start and stop

    Used for sorting static keys section and iterating stop signs.
* Approaches to keep custom section retained from linker's GC
* Approaches to bypass DEP

    Used to update static branch instructions.

## What arch-specific features are required to extend to new architectures?

* A `nop` instruction with same length as `jmp` (or can divide the length, e.g. 2-byte `nop` and 4-byte `jmp`)
* Approaches to clear instruction cache in Linux. (Should be added to [Evian-Zhang/clear-cache](https://github.com/Evian-Zhang/clear-cache))
* Inline assembly supported by Rust

## Can I use this crate in `no_std`?

Yes.

## How can I use this crate in bare metal?

You should modify your linker script to add `__start` and `__stop` prefix symbols for marking start and end address of corresponding sections. For more details, see [Evian-Zhang/static-keys#6](https://github.com/Evian-Zhang/static-keys/pull/6).
