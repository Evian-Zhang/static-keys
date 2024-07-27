# FAQs

## Why static key should be applied to seldomly changed features?

Two reasons:

* The modification of static key will impose a potential security risk since we need to bypass DEP. The DEP will be restored after modification done.
* The modification is less efficient since many syscalls are involved.

## Why static keys must only be modified in a single-thread environment?

Unlike Linux kernel, we cannot hold a memory lock in userland, which prevent other threads to execute the instructions being modified.

Moreover, we need to manipulate memory protection to bypass DEP, which may involves race condition on the protection itself in multi-thread environment.

## Why is nightly Rust required?

We use inline assembly internally, where `asm_goto` and `asm_const` feature is required. As long as these two features are stablized, we can use stable Rust then.

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
* Inline assembly supported by Rust

## Can I use this crate in `no_std`?

Yes.
