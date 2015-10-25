# NASM

Run NASM during your Cargo build.

```rust
extern crate gcc;

fn main() {
    nasm::compile_library("libfoo.a", &["foo.s", "bar.s"]);
}
```
