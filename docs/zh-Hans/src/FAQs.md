# FAQs

## 为什么static key应当被用在较少改变的特性上？

两点原因：

* 对static key的修改需要绕过DEP，会带来潜在的安全风险。不过DEP会在修改结束后重新生效。
* 对static key的修改比较慢，因为涉及到了许多系统调用。

## 为什么static key必须在单线程环境下修改？

在用户态，如果要修改别的线程可能会执行到的指令会非常复杂。Linux内核社区曾经提出过[`text_poke`系统调用](https://lwn.net/Articles/574309/)，但是如今仍不可用。顺带一提，[Linus好像不太喜欢这个](https://lore.kernel.org/lkml/CA+55aFzr9ZKcGfT_Q31T9_vuCcmWxGCh0wixuZqt7VhjxxYU9g@mail.gmail.com/)，并且他说的很有道理。

另一个原因是我们需要操作内存保护权限来绕过DEP，但是在多线程环境下，这会引发保护权限本身的race condition。尽管可以用mutex来解决数据竞争的问题，但是如果cargo解析出多版本的static-keys依赖，那么每个版本中都会有一个全局mutex实例，这种方法就失效了。这个可以被[RFC 1977: public & private dependencies](https://github.com/rust-lang/rust/issues/44663)解决。[rust-lang/cargo#2363](https://github.com/rust-lang/cargo/issues/2363)亦可供参考。

## 为什么需要nightly Rust？

我们在内部使用了内联汇编，并且使用了`asm_goto`和`asm_const`这两个特性。只要这两个特性稳定了，我们就能使用stable Rust了。

## 为什么`static_branch_likely!`和`static_branch_unlikely!`是宏？

因为内联汇编的`sym`参数需要是静态路径，这在函数里是做不到的。

## 如果要扩展到新的操作系统，需要实现哪些操作系统特性？

* 标志一个自定义节的开始和结束的符号

    这是用来对static key排序，以及标志循环结束
* 保证自定义节不会被链接器的GC回收的方案
* 绕过DEP的方案

    用于更新static branch的指令
## 如果要扩展到新的指令集架构，需要实现哪些架构特性？

* 与`jmp`等长的`nop`指令（或者可以整除，如2字节`nop`与4字节`jmp`）
* 在Linux上清除指令缓存的方式（需加入到[Evian-Zhang/clear-cache](https://github.com/Evian-Zhang/clear-cache)）
* Rust支持的内联汇编

## 我可以在`no_std`环境中使用吗？

可以

## 我可以在裸金属环境中使用吗？

可以。需要修改linker script，在对应的节前后加上`__start`和`__stop`为前缀的相应符号。具体可见[Evian-Zhang/static-keys#6](https://github.com/Evian-Zhang/static-keys/pull/6)。
