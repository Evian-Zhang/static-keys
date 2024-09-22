# static-keys

[![Rust CI status](https://github.com/Evian-Zhang/static-keys/actions/workflows/ci.yml/badge.svg)](https://github.com/Evian-Zhang/static-keys/actions/workflows/ci.yml)
[![Crates.io Version](https://img.shields.io/crates/v/static-keys)](https://crates.io/crates/static-keys)
[![docs.rs](https://img.shields.io/docsrs/static-keys?logo=docs.rs)](https://docs.rs/static-keys)

[Static key](https://docs.kernel.org/staging/static-keys.html)是Linux内核中的一个底层机制，用于加速对很少改变的特性的条件判断检查。我们将这一特性迁移到了用户态Rust程序中，适用于Linux、macOS和Windows。（目前需要Nightly版本的Rust，原因请参见[FAQ](https://evian-zhang.github.io/static-keys/zh-Hans/FAQs.html#为什么需要nightly-rust)）

目前在CI中经过测试的支持平台包括：

* Linux

    * `x86_64-unknown-linux-gnu`
    * `x86_64-unknown-linux-musl`
    * `i686-unknown-linux-gnu`
    * `aarch64-unknown-linux-gnu`
    * `riscv64gc-unknown-linux-gnu`
    * `loongarch64-unknown-linux-gnu`
* macOS

    * `aarch64-apple-darwin`
* Windows

    * `x86_64-pc-windows-msvc`
    * `i686-pc-windows-msvc`

需要注意，如果使用cross-rs交叉编译`loongarch64-unknown-linux-gnu`平台，需要使用GitHub上的最新版cross-rs。更多细节可参见[Evian-Zhang/static-keys#4](https://github.com/Evian-Zhang/static-keys/pull/4)。

更详细的解释和FAQ可参见[GitHub Pages](https://evian-zhang.github.io/static-keys/zh-Hans/)([English version](https://evian-zhang.github.io/static-keys/en/)).

## 出发点

现代程序往往可以通过命令行选项或配置文件进行配置。在配置中开启或关闭的选项往往在程序启动后是不会被改变的，但由在整个程序中被频繁访问。

```rust,ignore
let flag = CommandlineArgs::parse();
loop {
    if flag {
        do_something();
    }
    do_common_routines();
}
```

尽管`flag`在程序初始化后不会被修改，在每次循环中仍然会执行`if`判断。在x86-64架构中，这一判断一般会被编译为`test`-`jnz`指令

```x86asm
    test    eax, eax           ; 检查eax寄存器是否为0
    jnz     do_something       ; 如果不为0，则跳转do_something
do_common_routines:
    ; 执行相关指令
    ret
do_something:
    ; 执行相关指令
    jmp     do_common_routines ; 跳转至do_common_routines
```

尽管`if`判断只是`test`-`jnz`指令，这还可以进一步加速。我们是不是可以把这个判断优化为`jmp`指令（不执行`do_something`分支）或`nop`指令（总是执行`do_something`分支）？这就是static keys的原理。简单来说，我们需要**动态修改指令**。在从命令行获得`flag`之后，我们根据`flag`的值将`if flag {}`动态修改为`jmp`或`nop`指令。

例如，如果用户给出的`flag`为`false`，那么生成的指令将被**动态修改**为下面的`nop`指令

```x86asm
    nop     DWORD PTR [rax+rax*1+0x0]
do_common_routines:
    ; 执行相关指令
    ret
do_something:
    ; 执行相关指令
    jmp     do_common_routines
```

如果`flag`为`true`，那么生成的指令将被修改为下面的无条件跳转指令

```x86asm
    jmp     do_something
do_common_routines:
    ; 执行相关指令
    ret
do_something:
    ; 执行相关指令
    jmp     do_common_routines
```

这里的指令就不会再有`test`和条件跳转指令了，而仅仅是一个`nop`（即不做任何事）或`jmp`指令。

尽管将`test`-`jnz`替换为`nop`提升的性能可能微乎其微，但是[Linux内核文档](https://docs.kernel.org/staging/static-keys.html#motivation)中描述了，如果这样的配置检查涉及全局变量，那么这样的替换可以减小缓存压力。并且在服务端程序中，这些配置有可能会通过`Arc`在多线程中共享，那么用`nop`或`jmp`就可以有更大的提升空间。

## 使用方式

目前需要nightly版本的Rust来使用这个库。在使用者的代码中，需要声明对unstable特性`asm_goto`的使用。

```rust
#![feature(asm_goto)]
```

首先，在`Cargo.toml`中加入相应依赖：

```toml
[dependencies]
static-keys = "0.5"
```

在`main`函数开头，需要调用[`static_keys::global_init`](https://docs.rs/static-keys/latest/static_keys/fn.global_init.html)进行初始化。

```rust
fn main() {
    static_keys::global_init();
    // 执行其他指令
}
```

随后需要定义一个static key来记录用户传入的flag，并根据用户传入的值来控制这个static key的值。

```rust
# use static_keys::define_static_key_false;
# struct CommandlineArgs {}
# impl CommandlineArgs { fn parse() -> bool { true } }
// FLAG_STATIC_KEY初始值为`false`
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

同一个static key可以在任意时刻多次更改值。但需要注意的是，**在多线程环境中修改static key是非常危险的**。因此，如果需要使用多线程，请在完成对static key的修改后再创建新线程。不过，**在多线程环境中使用static key是绝对安全的**。此外，对static key的修改相对比较慢，但由于static key一般用于控制很少被修改的特性，所以这样的修改相对比较少，因此慢点也没有太大影响。请参见[FAQ](https://evian-zhang.github.io/static-keys/zh-Hans/FAQs.html#为什么static-key必须在单线程环境下修改)了解更多。

在定义static key之后，就可以像平常一样用`if`语句来使用这个static key了（[这个](https://doc.rust-lang.org/std/intrinsics/fn.likely.html)和[这个](https://kernelnewbies.org/FAQ/LikelyUnlikely)介绍了`likely`和`unlikely` API的语义）。同一个static key可以在多个`if`语句中被使用。当这个static key被修改时，所有使用这个static key的`if`语句都将被统一修改为`jmp`或`nop`。

```rust
# #![feature(asm_goto)]
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

## 参考链接

* Linux内核官方文档：[static-keys](https://docs.kernel.org/staging/static-keys.html)
* [Linux `static_key` internals](https://terenceli.github.io/%E6%8A%80%E6%9C%AF/2019/07/20/linux-static-key-internals)
* Rust for Linux项目也实现了static key，请查看[Rust-for-Linux/linux#1084](https://github.com/Rust-for-Linux/linux/pull/1084)。我们基于`break`的内联汇编实现是受到这一实现的启发的。
