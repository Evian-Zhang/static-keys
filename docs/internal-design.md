# Internal Design

As described in the introduction, the workflow of static keys is:

1. Globally initialize related structures.
2. Define a static key.
3. Modify the static key according to user-specified values.
4. Use the static key at the `if`-check.

In this section, we will use three terms to make things more clear:

* Static key

    A static variable storing information to control the enabling of static branches.
* Jump entry

    A static variable storing information about static branches. This is used to locate the static branch.
* Static branch

    A `if`-check which utilizes static keys.

## Simplified Logics

In simplified logics, we can think of the static key and jump entry as the following structures:

```rust, ignore
struct StaticKey {
    enabled: bool,
    jump_entries: Vec<JumpEntry>,
}

struct JumpEntry {
    code: &'static Instruction,
}
```

When we modify a static key, the following things happen:

1. The static key's `enable` field is modified
2. We find all jump entries associated with this static key by `jump_entries` field
3. For each jump entry, we locate the static branch by `static_branch` field
4. We modify the static branch to `nop`/`jmp` according to the `enable` value

And when we use a static key at `if`-check, we just push another jump entry into the static key, which records the location of current `if`-check.

After understanding the simplified logics, we can then add some more comprehensive supplements to the simplified logic:

* Jump entry location.
* Static branch modification content.
* Static branch modification rule.
* Static branch modification approach.

## Jump Entry Location

As described in Usage, we can use one static key at multiple `if` checks. As a result, one static key may be associated with multiple jump entries. However, we cannot construct a compile-time vector in ad-hoc: we cannot define a static vector, and push to this vector at compile time across the crate. As a result, we must store the jump entries in the generated binary, and construct the static key's jump entries at run time to collect associated jump entries.

In practice, we store these jump entries in an individual section in generated binary. The name of such section differs on each OS. For example, in Linux ELF, this section is named `__static_keys`.

Then at runtime, when initializing, we will collect jump entries to each static key. However, as the jump entries have already in a section loaded into memory, we don't want to double the memory usage to push those jump entries content into the static key's vector.

The solution is to make the `jump_entries` field of `StaticKey` a pointer instead of vector. It can just point to the jump entries in the individual section, thus decrease the memory usage. To do so, we then must sort the jump entries in such section to make sure jump entries associated with same static key are adjacent to each other, and then the `jump_entries` field can point to the first jump entry which associated with the static key.

To make the sort work, we should add another field to the `JumpEntry`: the address of static key. Then the sort can conduct according to static key address. Note that in implementaion, such addresses are all relative due to ASLR.

As a result, now the structure should be written as

```rust, ignore
struct StaticKey {
    enabled: bool,
    jump_entries: *const JumpEntry,
}

struct JumpEntry {
    code: &'static Instruction,
    /// Relative address to static key
    key: usize,
}
```

The `jump_entries` field is `null` at beginning, and when initializing, this field is updated to point to the first jump entry which asscoiated with this static key.

## Static Branch Modification Content

When modifying a static branch, we may modify `nop` to `jmp`, or `jmp` to `nop`. In most architectures, the `nop` instruction can be of many byte length. For example, in x86-64, since the `jmp` is usually 5-byte long, we select a 5-byte `nop` to do the replacement. This can be used to make sure that we do not mess up with the following instruction.

However, when modifying `nop` to `jmp`, which target should be jumped to? This cannot be deduced trivially. As a result, we need another field in `JumpEntry` to record the address of jump target:

```rust
struct JumpEntry {
    code: &'static Instruction,
    /// Relative address to jump target
    target: usize,
    key: usize,
}
```

To construct such jump entry at static branch, we use the following inline assembly instruction (take x86-64 for example). When using `static_branch_likely!` and `static_branch_unlikely`, the following code snippet will be generated (details may be different).

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

It seems complicated, and let me break it down to explain.

### Assembly part

The first line `2:` indicate an assembly label, which used to mark the location of current instruction: `0x0f, 0x1f, 0x44, 0x00, 0x00`, which is a 5-byte `nop` instruction.

Then we use `.pushsection` and `.popsection` pair to switch to another section (current section is `.text`, which is used to record instructions), which is used to store the jump entries.

Inside the new section, we use three `.quad` to define three 8-byte values, which corresponds to three fields of `JumpEntry` struct. The first 8-byte value is `2b - .`, where `2b` indicates the nearest label with name `2`, which is the address of `nop` instruction we just defined. The `.` represents current location, which is the address of this 8-byte value. Using `2b - .` to indicate a relative address to the `nop` instruction, which is the `code` field of `JumpEntry`.

The second 8-byte value is `{0} - .`, where `{0}` indicates the first argument in this inline assembly, which is `label { break 'my_label false; }`. This is the target address of `jmp` instruction, which corresponds to the `target` field of `JumpEntry`. This will be explained later.

The third 8-byte value is `{1} + {2} - .`, which stores information about static key and initial status of static branch (note that static key is always 8-byte aligned, so the last byte of its address is always `0x00`, which allows us to use this to record additional information). The initial status will be explained later as well.

By conducting this inline assembly, a jump entry will be generated in "__static_keys" section at compile time.

### Jump label part

Since the inline assembly does not affect the control flow, let's simply the code snippet to see more clear about jump label part:

```rust, ignore
'my_label {
    // Some inline-assembly
    break 'my_label true;
}
```

This will be treated as a `true` value expression by Rust compiler. Since we use the `static_branch_likely!` and `static_branch_unlikely!` in the `if`-check, the `if`-check then become

```rust, ignore
if true {
    do_a();
} else {
    do_b();
}
do_c();
```

As a result, the Rust compiler will optimize the instruction to be

```x86asm
nop        ; 0x0f,0x1f,0x44,0x00,0x00
call do_a  ; do_a()
```

However, `do_b()` will not be optimized out: there is an argument in the inline assembly which reference it --- the `label { break 'my_label false; }`. As described above, this argument is used as an address to the statement `break 'my_label false;`. When fitting this statement into the `if`-check, it become a `false` condition. As a result, this statement is translated to a call to `do_b()`, which this call is never executed in the static control flow. To make it more clear, let's see what is the generated assembly look like:

```x86asm
    nop           ; 0x0f,0x1f,0x44,0x00,0x00
    call    do_a  ; do_a()
DO_C:
    call    do_c  ; do_c()
    ret           ; End of this function
DO_B:
    call    do_b  ; do_b()
    jmp     DO_C  ; goto DO_C
```

The basic block at `DO_B` will never be executed in the static control flow, while we do pass its address to a jump entry stored in the individual section.

When we modify the static branch to a `jmp`, the assembly become:

```x86asm
    jmp     DO_B  ; Modified!
    call    do_a  ; do_a()
DO_C:
    call    do_c  ; do_c()
    ret           ; End of this function
DO_B:
    call    do_b  ; do_b()
    jmp     DO_C  ; goto DO_C
```

Things go right!

## Static Branch Modification Rule

### Branch layout

As described in the static branch modification content, there are two branches to be executed: one can be executed after `nop`, and is adjacent to the main main part; another shall be executed with two additional `jmp`, and its location is in the end of function. This difference will make a little impact on the performance. Usually, the branch that unlikely to be executed should be the latter one, and the other should be the former one. In this crate, the layout is controlled by `static_branch_likely!` and `static_branch_unlikely!`.

When using `static_branch_likely!`, the `true` branch will become a likely branch, which will be positioned near the main part, and can be just `nop`ed to it. The `false` branch is positioned in some other places, and is involved with two additional `jmp`s.

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
