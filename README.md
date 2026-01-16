# Unsafe Memory Corruption Demo

A Rust program that visualizes how `unsafe` code can corrupt memory and break safe code's assumptions. Works in both the Rust Playground and local terminals.

## What this is

This is a teaching tool. Specifically, it's a tool for watching memory get corrupted in slow motion and then observing safe code fall over in confusion because someone lied to it.

If that sounds like a strange thing to want, well, welcome to systems programming education.

## The lesson

The core insight we're trying to demonstrate:

1. Safe Rust code makes assumptions ("this `len` field is ≤ 5, I can slice with it")
2. `unsafe` code can violate those assumptions (write a `9` into the `len` bytes)
3. Safe code then panics, and the backtrace points at *safe* code
4. The actual bug was three functions away, in an `unsafe` block

This is why `unsafe` audits are so important: the symptom and the cause live in different zip codes.

## The journey; or, how we learned to stop trusting the stack

### Our first attempt

Our initial approach was charmingly naive:

```rust
fn demo() {
    let mut buffer: [u8; 5] = [0u8; 5];
    let num: i32 = 40_000;  // surely this lives "after" buffer, right?
    let ptr = buffer.as_mut_ptr();

    // Let's just... read 16 bytes starting at buffer
    let snapshot = unsafe { std::slice::from_raw_parts(ptr, 16) };

    // And write past the end
    unsafe { *ptr.add(8) = 0x42; }

    println!("num = {}", num);  // Corrupted! ...right?
}
```

This has problems. Several of them, actually.

### Problem 1: The observer is the observed (and both are undefined)

Here's a fun paradox: our "memory snapshot" function was reading 16 bytes from a 5-byte buffer. That's undefined behavior. We were committing UB just to *look* at the corruption we were about to cause.

The compiler is within its rights to assume UB never happens. So our snapshot could contain anything. Stale values. Garbage. The lyrics to a song the compiler likes. The output is "not something Rust promises to be meaningful or repeatable," as ChatGPT put it.

We wanted to observe memory corruption, but our observation method was itself corrupted. It's like trying to measure voltage with a multimeter whose probes are also on fire.

### Problem 2: Stack layout is a polite fiction

We assumed `num` would live "right after" `buffer` in memory. Rust makes no such promise. The compiler can:

- Reorder locals for alignment
- Keep values in registers (never touching memory)
- Merge variables, split variables, eliminate variables entirely
- Do whatever it wants, really; it's the compiler's stack

Our "offset calculation" was measuring the distance between two points that might not exist:

```rust
// This number is meaningless. Stack layout is unspecified.
let offset = (num_ptr as usize) - (buffer_ptr as usize);
```

### Problem 3: The compiler is smarter than us

Even if we *did* successfully overwrite `num`'s memory location, the compiler might have cached the value in a register. It "knows" that `num` was set to `40_000` and never modified through a valid reference, so why bother re-reading it?

Result: `println!("{}", num)` prints `40000` even though we "corrupted" it. The corruption happened, in some sense, but the compiler optimized away our ability to see it.

### The solution: one allocation to rule them all

The fix is to stop pretending separate stack variables are part of the same memory region and actually *make* them part of the same memory region:

```rust
#[repr(C)]
struct Frame {
    buffer: [u8; 5],
    len: UnsafeCell<u32>,
    num: UnsafeCell<i32>,
    guard: UnsafeCell<u32>,
}
```

Now everything lives in one allocation. We can legally read any byte within `size_of::<Frame>()`. The layout is specified (thanks, `repr(C)`). The observation is sound even if what we're observing is unsound.

## A brief tour of the Rust APIs we're abusing

### `#[repr(C)]`: "please just put things where I tell you"

Rust's default struct layout is unspecified. Fields can go anywhere. `#[repr(C)]` forces C-compatible layout: fields appear in declaration order, with predictable padding. This is essential because we need to know where things are to corrupt them precisely.

### `UnsafeCell<T>`: "I know what I'm doing" (narrator: he didn't)

We're going to modify `len` through raw pointers while also reading it through `&self`. Without `UnsafeCell`, this violates aliasing rules; the compiler assumes shared references don't observe mutation, and we're about to violate that assumption aggressively.

`UnsafeCell` is the escape hatch. It tells the compiler "this data may change even through shared references." It's the primitive underlying `Cell`, `RefCell`, `Mutex`, and every other interior mutability type in Rust.

### `read_volatile()`: "no really, read from memory this time"

The compiler might "know" we set `len = 5` and optimize away subsequent reads. `read_volatile` forces an actual memory access. It's typically used for memory-mapped I/O, but here we're using it to defeat the optimizer's entirely reasonable assumption that we're not monsters.

### `offset_of!`: compile-time field offsets

Rather than computing offsets at runtime (which could vary based on... well, anything), we use the standard library macro to get stable, compile-time values. Combined with `repr(C)`, this gives us predictable results across runs.

### `AssertUnwindSafe`: "trust me, it's fine"

`catch_unwind` requires "unwind safe" closures. `UnsafeCell` isn't unwind safe because it might be mid-mutation when a panic occurs. We use `AssertUnwindSafe` to promise we'll handle this correctly.

We are lying, of course. But it's a small lie in service of education.

## The memory layout

For those who prefer pictures (or at least ASCII approximations of pictures):

```
Offset  Field         Bytes (example)
------  -----         ---------------
0-4     buffer[0..5]  [00][01][02][03][04]
5-7     (padding)     [??][??][??]
8-11    len           [05][00][00][00]  <- 5 in little-endian
12-15   num           [40][9c][00][00]  <- 40,000 = 0x9C40
16-19   guard         [ef][be][ad][de]  <- 0xDEADBEEF, our canary
```

When we write bytes 0, 1, 2, ... starting at `buffer[0]`:

| Write index | What happens |
|-------------|--------------|
| 0-4 | Safe. We're in `buffer`. |
| 5-7 | We hit padding. Nothing visible breaks. |
| 8-11 | We corrupt `len`. It becomes `0x0B0A0908`. |
| 12-15 | We corrupt `num`. |
| 16-19 | We corrupt `guard`. The canary dies. |

When safe code later does `buffer[..len]` with `len = 185207048`, it panics. The bounds check fails. Safe code did nothing wrong; it just trusted a liar.

## Running the demo

### Locally

```bash
cargo run
```

Colors in terminal; text markers when piped. The `Cargo.toml` disables optimizations (more on that below).

### Rust Playground

[Run it directly in the Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2024&gist=3b93adfff6fc81bdfa99110402b136f8)

Or manually:

1. Paste the contents of `src/main.rs`
2. **Important**: Click the three-dot menu, then Build options, then select **Debug** mode (or set opt-level to 0)
3. Run

### Why opt-level = 0?

With optimizations enabled, the compiler may:

- Keep values in registers, never reading the "corrupted" memory
- Cache values it "knows" couldn't have changed
- Reorder or eliminate code based on the assumption that UB didn't happen

At opt-level 0, the compiler generates straightforward code that actually reads from memory. The corruption becomes visible. We are, in a sense, handicapping the compiler so it can't outsmart our bugs.

This is also why the demo uses `read_volatile`: belt and suspenders.

## What this proves (and doesn't)

### What it demonstrates

- Memory is just bytes; overwrite them and values change
- `repr(C)` gives predictable layout
- Safe code trusts invariants; violating them causes chaos
- The crash site and the crime scene are different places

### What it doesn't demonstrate

- **Real exploits** target return addresses, vtables, function pointers; not i32s
- **UB is not deterministic**: our demo is carefully constructed; real UB can do anything
- **This isn't a fuzzer**: we're corrupting known offsets, not discovering vulnerabilities

## Further reading

- [The Rustonomicon](https://doc.rust-lang.org/nomicon/): Rust's official guide to dark magic
- [UnsafeCell docs](https://doc.rust-lang.org/std/cell/struct.UnsafeCell.html): the primitive at the bottom of the rabbit hole
- [Type layout reference](https://doc.rust-lang.org/reference/type-layout.html#reprc): `repr(C)` and friends

## Acknowledgments

- Multiple conversations with ChatGPT and Claude Code, to play devil's advocate.

Thanks to Daniel Cumming, who answered numerous questions on Slack that led to the insight that our initial approach was "mostly a UB generator with a pretty printer around it." His guidance gave us proper mental models and clear boundaries of understanding. This version is a *well-defined* pretty printer around a *localized* UB generator, which is a meaningful improvement.
