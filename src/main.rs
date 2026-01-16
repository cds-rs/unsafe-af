//! # Unsafe Memory Corruption Demo
//!
//! This demo shows how `unsafe` code can bypass Rust's safety guarantees and corrupt
//! data that safe code depends on. It's designed to be educational and work in both
//! the Rust Playground and local terminals.
//!
//! ## What This Demonstrates
//!
//! 1. Safe Rust code relies on invariants (e.g., "len <= buffer.len()")
//! 2. Unsafe code can violate these invariants by writing out of bounds
//! 3. When safe code later runs, it trusts the corrupted data and panics/misbehaves
//!
//! ## Why This Matters
//!
//! In real programs, this kind of bug can lead to:
//! - Security vulnerabilities (buffer overflows)
//! - Data corruption
//! - Undefined behavior
//! - Hard-to-debug crashes far from the actual bug

// ============================================================================
// COLOR OUTPUT MODULE
// ============================================================================

/// Provides colored/marked output for the hex dump visualization.
///
/// Automatically detects if stdout is a terminal:
/// - Terminal: uses ANSI escape codes for red (changed) and green (watched)
/// - Not a terminal (playground, pipe, file): uses [brackets] and (parens)
mod color {
    use std::io::{stdout, IsTerminal};
    use std::sync::OnceLock;

    /// Cached result of terminal detection (checked once at startup)
    fn use_ansi() -> bool {
        static IS_TTY: OnceLock<bool> = OnceLock::new();
        *IS_TTY.get_or_init(|| stdout().is_terminal())
    }

    /// Format a byte that changed this iteration (red / [bracketed])
    pub fn red(byte: u8) -> String {
        if use_ansi() {
            format!("\x1b[91m{byte:02x}\x1b[0m")
        } else {
            format!("[{byte:02x}]")
        }
    }

    /// Format a watched byte that hasn't been corrupted yet (green / (parens))
    pub fn green(byte: u8) -> String {
        if use_ansi() {
            format!("\x1b[92m{byte:02x}\x1b[0m")
        } else {
            format!("({byte:02x})")
        }
    }

    /// Format a plain byte (no special highlighting)
    pub fn plain(byte: u8) -> String {
        format!(" {byte:02x} ")
    }
}

use std::cell::UnsafeCell;
use std::mem::{offset_of, size_of};
use std::panic::AssertUnwindSafe;

// ============================================================================
// THE FRAME STRUCT - Our "victim" data structure
// ============================================================================

const BUFFER_SIZE: usize = 5;

/// A contiguous memory region with a known, stable layout.
///
/// # Why `#[repr(C)]`?
///
/// Rust's default struct layout is unspecified - the compiler can reorder fields
/// for efficiency. `#[repr(C)]` forces C-compatible layout: fields appear in
/// declaration order with predictable padding. This lets us know exactly where
/// each field lives in memory.
///
/// # Why `UnsafeCell`?
///
/// We're going to modify `len`, `num`, and `guard` through raw pointers while
/// also reading them through `&self`. Without `UnsafeCell`, this would be
/// undefined behavior (violating Rust's aliasing rules). `UnsafeCell` tells
/// the compiler "this data may be mutated through shared references."
///
/// # The Invariant
///
/// Safe code assumes: `len <= BUFFER_SIZE` (so `buffer[..len]` is valid)
/// Unsafe code will violate this by overwriting `len` with garbage.
#[repr(C)]
struct Frame {
    /// The buffer we're "supposed" to write into
    buffer: [u8; BUFFER_SIZE],

    /// Length field - safe code trusts this to be <= BUFFER_SIZE
    len: UnsafeCell<u32>,

    /// Another value safe code might depend on
    num: UnsafeCell<i32>,

    /// Sentinel value (0xDEAD_BEEF) - makes corruption visually obvious
    guard: UnsafeCell<u32>,
}

impl Frame {
    /// Create a new Frame with valid initial state
    fn new() -> Self {
        Self {
            buffer: [0u8; BUFFER_SIZE],
            len: UnsafeCell::new(BUFFER_SIZE as u32), // Valid: len == buffer.len()
            num: UnsafeCell::new(40_000),
            guard: UnsafeCell::new(0xDEAD_BEEF), // Easy to spot if corrupted
        }
    }

    /// Read `len` using volatile to prevent compiler optimizations.
    ///
    /// # Why volatile?
    ///
    /// The compiler might "know" that len was set to 5 and optimize away
    /// the read. Volatile forces an actual memory read, so we see the
    /// corrupted value after our unsafe writes.
    #[inline(always)]
    fn read_len_volatile(&self) -> u32 {
        unsafe { std::ptr::read_volatile(self.len.get()) }
    }

    #[inline(always)]
    fn read_num_volatile(&self) -> i32 {
        unsafe { std::ptr::read_volatile(self.num.get()) }
    }

    #[inline(always)]
    fn read_guard_volatile(&self) -> u32 {
        unsafe { std::ptr::read_volatile(self.guard.get()) }
    }
}

// ============================================================================
// MEMORY VIEW - Visualization of memory changes
// ============================================================================

/// Tracks memory snapshots and highlights changes between iterations.
///
/// Generic over `N` (the size of the memory region to track).
struct MemoryView<const N: usize> {
    /// Current snapshot of memory
    snapshot: [u8; N],

    /// Which bytes have been corrupted (changed at least once)
    corrupted: [bool; N],

    /// Byte ranges to highlight as "watched" (e.g., the len/num/guard fields)
    watched_ranges: &'static [(usize, usize)],

    /// Byte positions where we print a "|" separator for readability
    separators: &'static [usize],
}

impl<const N: usize> MemoryView<N> {
    fn new(watched_ranges: &'static [(usize, usize)], separators: &'static [usize]) -> Self {
        Self {
            snapshot: [0u8; N],
            corrupted: [false; N],
            watched_ranges,
            separators,
        }
    }

    /// Copy N bytes from memory into our snapshot
    fn capture(&mut self, base_ptr: *const u8) {
        unsafe {
            std::ptr::copy_nonoverlapping(base_ptr, self.snapshot.as_mut_ptr(), N);
        }
    }

    /// Should we print a separator before this byte index?
    fn is_separator(&self, i: usize) -> bool {
        self.separators.contains(&i)
    }

    /// Is this byte in one of the watched ranges?
    fn is_watched(&self, i: usize) -> bool {
        self.watched_ranges
            .iter()
            .any(|&(start, end)| i >= start && i < end)
    }

    /// Print a single byte with appropriate formatting
    fn print_byte(&self, i: usize, byte: u8, changed_this_iter: bool) {
        if self.is_separator(i) {
            print!(" |");
        }

        let formatted = if changed_this_iter {
            color::red(byte) // Just changed - highlight in red
        } else if self.is_watched(i) && !self.corrupted[i] {
            color::green(byte) // Watched and pristine - highlight in green
        } else {
            color::plain(byte) // Plain or already corrupted
        };

        print!("{formatted}");
    }

    /// Print current snapshot with a label (no diff highlighting)
    fn print_row(&self, label: &str) {
        print!("{label:<6} |");
        for (i, &byte) in self.snapshot.iter().enumerate() {
            self.print_byte(i, byte, false);
        }
        println!();
    }

    /// Print current snapshot, highlighting differences from `prev`
    fn print_diff(&mut self, prev: &[u8; N], label: &str) {
        print!("{label:<6} |");
        for (i, (&p, &c)) in prev.iter().zip(self.snapshot.iter()).enumerate() {
            self.print_byte(i, c, p != c);
        }
        println!();

        // Mark any changed bytes as corrupted for future iterations
        for i in 0..N {
            if prev[i] != self.snapshot[i] {
                self.corrupted[i] = true;
            }
        }
    }
}

// ============================================================================
// SAFE CODE THAT TRUSTS THE INVARIANT
// ============================================================================

/// Sum the first `len` bytes of the buffer.
///
/// # The Problem
///
/// This function is 100% safe Rust - no `unsafe` keyword anywhere.
/// It trusts that `frame.len` is a valid length (<= BUFFER_SIZE).
///
/// But if unsafe code corrupted `len` to be larger than BUFFER_SIZE,
/// the slice `buffer[..len]` will panic with an out-of-bounds error.
///
/// This demonstrates: **unsafe code can break safe code's assumptions.**
fn safe_sum_prefix(frame: &Frame) -> u64 {
    let len = frame.read_len_volatile() as usize;

    // This line will PANIC if len > BUFFER_SIZE
    // The bounds check is done by safe Rust, but it fails because
    // unsafe code corrupted the `len` field.
    frame.buffer[..len].iter().map(|&b| b as u64).sum()
}

// ============================================================================
// MAIN - Run the demonstration
// ============================================================================

fn main() {
    // ========================================================================
    // STEP 1: Calculate struct layout at compile time
    // ========================================================================

    // offset_of! gives us the byte offset of each field within Frame.
    // This is stable because we used #[repr(C)].
    const BUF_OFF: usize = offset_of!(Frame, buffer);
    const LEN_OFF: usize = offset_of!(Frame, len);
    const NUM_OFF: usize = offset_of!(Frame, num);
    const GUARD_OFF: usize = offset_of!(Frame, guard);

    const LEN_SZ: usize = size_of::<u32>();
    const NUM_SZ: usize = size_of::<i32>();
    const GUARD_SZ: usize = size_of::<u32>();

    const FRAME_SIZE: usize = size_of::<Frame>();

    // ========================================================================
    // STEP 2: Configure the memory view visualization
    // ========================================================================

    // These are the byte ranges we want to highlight (the "important" fields)
    const WATCHED: &[(usize, usize)] = &[
        (LEN_OFF, LEN_OFF + LEN_SZ),     // len field
        (NUM_OFF, NUM_OFF + NUM_SZ),     // num field
        (GUARD_OFF, GUARD_OFF + GUARD_SZ), // guard field
    ];

    // Where to draw vertical separators in the hex dump
    const SEPS: &[usize] = &[
        BUF_OFF + BUFFER_SIZE, // After buffer
        LEN_OFF,               // Before len (if there's padding)
        NUM_OFF,               // Before num
        GUARD_OFF,             // Before guard
    ];

    // ========================================================================
    // STEP 3: Print the struct layout
    // ========================================================================

    println!("=======================================================");
    println!("   UNSAFE MEMORY CORRUPTION DEMO");
    println!("=======================================================\n");

    println!("Frame struct layout (all offsets in bytes):");
    println!("  buffer: [{}..{}), size = {} bytes", BUF_OFF, BUF_OFF + BUFFER_SIZE, BUFFER_SIZE);
    println!("  len:    [{}..{}), size = {} bytes", LEN_OFF, LEN_OFF + LEN_SZ, LEN_SZ);
    println!("  num:    [{}..{}), size = {} bytes", NUM_OFF, NUM_OFF + NUM_SZ, NUM_SZ);
    println!("  guard:  [{}..{}), size = {} bytes", GUARD_OFF, GUARD_OFF + GUARD_SZ, GUARD_SZ);
    println!("  Total Frame size = {} bytes\n", FRAME_SIZE);

    println!("Legend:");
    println!("  (xx) = watched field, not yet corrupted");
    println!("  [xx] = byte changed this iteration");
    println!("   xx  = plain byte\n");

    // ========================================================================
    // STEP 4: Run the demo with increasing write lengths
    // ========================================================================

    for end in [5, 6, 8, 10, 12] {
        // Create a fresh Frame for each test
        let mut frame = Frame::new();
        let base_ptr: *mut u8 = (&mut frame as *mut Frame).cast::<u8>();

        // Set up memory view for this iteration
        let mut view: MemoryView<FRAME_SIZE> = MemoryView::new(WATCHED, SEPS);
        view.capture(base_ptr);

        println!("───────────────────────────────────────────────────────");
        println!("TEST: Write {} bytes starting at buffer[0]", end);
        println!("      (buffer is only {} bytes!)", BUFFER_SIZE);
        println!("───────────────────────────────────────────────────────");

        println!(
            "Before: len={}, num={}, guard=0x{:08X}",
            frame.read_len_volatile(),
            frame.read_num_volatile(),
            frame.read_guard_volatile()
        );

        let mut prev = view.snapshot;
        view.print_row("init");

        // ====================================================================
        // THE DANGEROUS PART: Unsafe writes with no bounds checking
        // ====================================================================
        //
        // This loop writes bytes 0, 1, 2, ... starting at buffer[0].
        // When `i >= BUFFER_SIZE`, we're writing past the buffer into
        // the `len`, `num`, and `guard` fields!
        //
        // This is the core teaching moment:
        // - Safe Rust would never allow buffer[5] on a 5-element array
        // - But with raw pointers in unsafe, there's no bounds check
        // - We just overwrite whatever memory comes next
        //
        unsafe {
            let buf_ptr = base_ptr.add(BUF_OFF);

            for i in 0..end {
                // This write has NO BOUNDS CHECK.
                // For i >= 5, we're corrupting adjacent fields!
                *buf_ptr.add(i) = i as u8;

                // Capture and display the memory state after each write
                view.capture(base_ptr);
                view.print_diff(&prev, &format!("i={i}"));
                prev = view.snapshot;
            }
        }

        // ====================================================================
        // Show the damage
        // ====================================================================

        println!(
            "After:  len={}, num={}, guard=0x{:08X}",
            frame.read_len_volatile(),
            frame.read_num_volatile(),
            frame.read_guard_volatile()
        );

        // ====================================================================
        // Demonstrate safe code breaking
        // ====================================================================
        //
        // safe_sum_prefix() is 100% safe Rust code.
        // But it trusts that `len` is valid.
        // If we corrupted `len` to be > 5, it will panic on bounds check.
        //
        let safe_result = std::panic::catch_unwind(AssertUnwindSafe(|| safe_sum_prefix(&frame)));
        match safe_result {
            Ok(sum) => println!("safe_sum_prefix() = {} (len was still valid)", sum),
            Err(_) => println!("safe_sum_prefix() PANICKED! (len was corrupted to > {})", BUFFER_SIZE),
        }

        println!();
    }

    // ========================================================================
    // SUMMARY
    // ========================================================================

    println!("=======================================================");
    println!("   KEY TAKEAWAYS");
    println!("=======================================================");
    println!();
    println!("1. Safe Rust code relies on invariants (len <= buffer size)");
    println!("2. Unsafe code can violate these invariants");
    println!("3. When safe code runs later, it trusts the corrupted data");
    println!("4. This leads to panics, crashes, or security vulnerabilities");
    println!();
    println!("This is why `unsafe` requires careful review:");
    println!("  - The bug is in the unsafe block");
    println!("  - But the crash happens in safe code!");
    println!("  - This makes debugging very difficult");
}
