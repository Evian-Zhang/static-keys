# 底层实现

正如在简介中所说，static key的使用流程如下：

1. 全局初始化相应结构。
2. 定义一个static key。
3. 根据用户传入的值修改static key。
4. 在`if`判断处使用static key。

在本节中，我们使用如下术语：

* Static key

    静态变量，用于存储相关信息来控制static branch的选取。
* Jump entry

    静态变量，用于存储static branch的信息。这个变量用于定位static branch。
* Static branch

    使用static key的`if`判断。

## 简化逻辑

在简化的逻辑中，我们可以把static key和jump entry理解为以下结构体：

```rust, ignore
struct StaticKey {
    enabled: bool,
    jump_entries: Vec<JumpEntry>,
}

struct JumpEntry {
    code: &'static Instruction,
}
```

当我们修改static key时，会进行以下的步骤：

1. 修改static key的`enable`字段
2. 根据`jump_entries`字段去找所有与这个static key相关联的jump entry
3. 对于每个jump entry，根据其`static_branch`字段来定位static branch
4. 根据`enable`字段的值来修改static branch为`nop`或`jmp`

我们在`if`判断处使用static key，就是在static key的`entries`字段增加一个元素，记录当前`if`判断的位置。

在理解了简化逻辑之后，我们还需要以下补充：

* jump entry的位置
* static branch的修改内容
* static branch的修改准则
* static branch的修改方式

## jump entry的位置

根据之前介绍的使用方式，我们可以在多处`if`判断中使用同一个static key。因此，一个static key可能会与多个jump entry相关联。但是，我们不能分布式地创建一个编译期的vector：我们无法定义一个静态vector之后，在各处代码中在编译期往这个vector里加入元素。因此，我们必须将jump entry存储在生成的二进制文件中，在运行时把jump entry与static key相关联。

具体来说，我们将jump entry存储在生成的二进制文件的特定节中。这个节的名称在不同操作系统中不同。例如，在Linux ELF中，我们将这个节称为`__static_keys`。

在运行时的初始化阶段，我们会收集每个static key关联的jump entry。但是，由于jump entry都位于一个已经载入内存的节中，所以如果再把这个jump entry加入static key的vector中，那么内存占用就会翻倍，这是我们不想看到的。

为了解决这个问题，我们把`jump_entries`字段定义为一个指针而非一个vector。这个指针可以直接指向相应节中的jump entry，因此可以减少内存占用。为了做到这样，我们需要对这个节中的jump entry进行排序，来确保相同的static key的jump entry应该相邻，这样的话`jump_entries`字段可以指向static key关联的第一个jump entry。

为了能够进行排序，我们需要在`JumpEntry`中再加入一个字段：static key的地址。这样的话，我们就可以根据这个地址对jump entry进行排序。需要注意到的一点是，在实现中，考虑到ASLR，这些地址都是相对地址。

因此，相应的结构体需要修改为

```rust, ignore
struct StaticKey {
    enabled: bool,
    jump_entries: *const JumpEntry,
}

struct JumpEntry {
    code: &'static Instruction,
    /// static key的相对地址
    key: usize,
}
```

`jump_entries`字段在一开始是`null`，然后在初始化时，这个字段被更新为指向其关联的第一个jump entry的指针。

## static branch的修改内容

当修改static branch时，我们需要把`nop`修改为`jmp`或把`jmp`修改为`nop`。在大多数指令集架构中，`nop`指令的长度可以有多种。例如，在x86-64架构中，由于`jmp`一般5字节，所以我们选择了5字节长度的`nop`来替换相应指令。这样可以保证我们不会污染相邻的指令。

但是，在修改`nop`为`jmp`时，我们该如何填`jmp`的目的地址？这并不能被很简单地计算出来。因此，我们需要在`JumpEntry`结构体中增加另一个字段来记录跳转的目的地址：

```rust
struct JumpEntry {
    code: &'static Instruction,
    /// 跳转目标的相对地址
    target: usize,
    key: usize,
}
```

为了在static branch处生成相应的jump entry，我们使用了如下的内联汇编（以x86-64为例）。当使用`static_branch_likely!`和`static_branch_unlikely!`宏时，会展开为如下代码片段（具体细节可能会有所差别）：

```rust, ignore
'my_label {
    ::core::arch::asm!(
        r#"
        2:
        .byte 0x0f,0x1f,0x44,0x00,0x00
        .pushsection __static_keys, "awR"
        .balign 8
        .quad 2b - .
        .quad {0} - .
        .quad {1} + {2} - .
        .popsection
        "#
        label {
            break 'my_label false;
        },
        sym MY_STATIC_KEY,
        const true as usize,
    );
    break 'my_label true;
}
```

这看上去非常复杂，我们来分段讲解。

### 汇编片段

第一行`2:`代表一个汇编标签，用于表示当前`0x0f, 0x1f, 0x44, 0x00, 0x00`这个数据的地址。这5个字节构成了一个`nop`指令。

然后我们使用一对`.pushsection`和`.popsection`来切换至另一个节（当前节为`.text`，用于记录指令），用于记录jump entry。

在新的节中，我们使用三个`.quad`，定义了三个8字节的值。这三个值分别对应为`JumpEntry`结构体的三个字段。第一个8字节值是`2b - .`，这里面`2b`代表与当前最近的`2`标签，也就是刚刚定义的`nop`指令的地址。而`.`代表当前的位置，也就是现在的8字节的值的地址。因此，`2b - .`就代表了一个与`nop`的相对地址，也就是`JumpEntry`的`code`字段。

第二个8字节值是`{0} - .`。这里`{0}`代表内联汇编的第一个参数，也就是`label { break 'my_label false; }`。这就是`jmp`指令的目的地址，也就对应于`JumpEntry`的`target`字段。这将在后面更详细地解释。

第三个8字节值是`{1} + {2} - .`，存储了static key的相应信息以及其初始值（需要注意的是，由于static key总是8字节对齐，因此其地址的最后一个字节总是`0x00`，所以我们就可以用这个字节去记录额外信息）。这个初始值我们也将在之后详细解释。

通过执行这个内联汇编，在"__static_keys"节就可以在编译期生成一个jump entry。

### 跳转标签部分

由于这里的内联汇编并不影响控制流，所以我们把上面的代码片段简化一下，只看其跳转标签部分：

```rust, ignore
'my_label {
    // 内联汇编
    break 'my_label true;
}
```

这段代码会被Rust编译器理解为一个`true`值。由于我们是在`if`判断处使用这些宏，所以`if`判断就变成了

```rust, ignore
if true {
    do_a();
} else {
    do_b();
}
do_c();
```

因此，Rust编译器就会将这些指令优化为

```x86asm
nop        ; 0x0f,0x1f,0x44,0x00,0x00
call do_a  ; do_a()
```

但是，`do_b()`并不会被优化掉：内联汇编的参数中用到了这个分支---`label { break 'my_label false; }`。正如前面所说，这个参数代表了`break 'label false;`语句的地址。当把这个语句放在`if`判断中时，就变成了一个`false`条件。因此，这个语句就会被编译为一个对`do_b()`的调用，而这个调用在静态控制流中是永远不会被执行的。为了理解得更清晰，我们来看看生成的汇编：

```x86asm
    nop           ; 0x0f,0x1f,0x44,0x00,0x00
    call    do_a  ; do_a()
DO_C:
    call    do_c  ; do_c()
    ret           ; 函数结尾
DO_B:
    call    do_b  ; do_b()
    jmp     DO_C  ; goto DO_C
```

`DO_B`处的基本块在静态控制流中永远不会被执行，而我们把它的地址存储在了jump entry中。

当我们把这个static branch修改为`jmp`，汇编代码就变成了

```x86asm
    jmp     DO_B  ; 此处被修改
    call    do_a  ; do_a()
DO_C:
    call    do_c  ; do_c()
    ret           ; 函数结尾
DO_B:
    call    do_b  ; do_b()
    jmp     DO_C  ; goto DO_C
```

一切符合预期。

## static branch的修改准则

### 分支布局

正如之前所介绍的，有两个分支会被执行：一个分支在`nop`后被执行，与主要的部分相邻。另一个分支需要通过两个额外的`jmp`执行，它的位置一般在函数的结尾。一般来说，不太可能被执行到的分支应当被放在后者，而前者则应当是更有可能被执行到的分支。这样的分支布局是通过`static_branch_likely!`和`static_branch_unlikely!`来控制的。

在使用`static_branch_likely!`时，更有可能被执行到的分支会放在`true`分支，也就是紧邻着主要部分，在执行完`nop`后就被执行。而`false`分支则被放在了其他位置，需要通过两个`jmp`来执行。

In the inline assembly, the difference is represented by `break 'my_label true` or `break 'my_label false` in the end of block.

### Initial instruction

After getting the right branch layout, then which instruction should be the initial instruction generated into the binary? It is used for the situation where, we do not update the static key, then its associated static branches need to take the correct path.

The rule is:

* For `static_branch_likely!`

    * If static key is defined with initial value `true`, then generate `nop`.
    * If static key is defined with initial value `false`, then generate `jmp`.
* For `static_branch_unlikely!`

    * If static key is defined with initial value `false`, then generate `nop`.
    * If static key is defined with initial value `true`, then generate `jmp`.

### Modification direction

Another question is, when enabling/disabling a static key, what instruction should we update to? Should we update `jmp` to `nop`, or update `nop` to `jmp`? To solve this question, we shall use the initial status recorded in the last byte of static key address in `key` field of `JumpEntry`.

The initial status is a bool, which indicates whether the likely branch is `true` branch. As described above, the likely branch should always be adjacent to the main part. And this status is controlled by whether we use `static_branch_likely!` or `static_branch_unlikely!`, and the initial value of static key.

Then when modifying static branches, the modification direction can be determined by `xor`ing the new value of static key, and the initial status recorded in jump entry. For example, if the likely branch is `true` branch, and the new value of static key is `true`, then we shall update `jmp` to `nop`, since we need to execute the block adjacent to the static branch check.

## Static Branch Modification Approach

The last question need to be solved, is how to modify static branch.

As a ground knowledge, the instructions are in text section. In most platforms, the text section has executable protection, and is non-writable to avoid attackers to modify instructions to execute malicious logic. This kind of protection mechanism is called DEP (Data Execution Protection) or W^X (Writable Xor eXecutable).

In order to modify static branch instructions, we then need to bypass the DEP in a short moment. This may be dangerous and vulnerable, while the DEP bypassing only happens in the modification of static key. After modification done, the protection is restored. So pay attention to the modification!
