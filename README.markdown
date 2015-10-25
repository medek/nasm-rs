# NASM

Run NASM during your Cargo build.

```rust
extern crate nasm;

fn main() {
    nasm::compile_library("libfoo.a", &["foo.s", "bar.s"]);
}
```
