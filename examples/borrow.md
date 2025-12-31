# Understanding the Rust Borrow Checker

The borrow checker ensures memory safety by enforcing that data has either one mutable reference or multiple immutable references, but never both. This prevents data races and ensures that memory is not accessed after it has been freed.

### Immutable Borrowing

Multiple parts of your code can read from the same data simultaneously as long as none of them can modify it.

```rust
let s = String::from("hello");
let r1 = &s;
let r2 = &s;
println!("{} and {}", r1, r2);
```
>> hello and hello

### Mutable Borrowing

To prevent data inconsistency, Rust allows only one active mutable reference to a piece of data at a time.

```rust
let mut s = String::from("hello");
let r1 = &mut s;
r1.push_str(", world");
println!("{}", r1);
```
>> hello, world
