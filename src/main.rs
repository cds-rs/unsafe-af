/// Demonstrating buffer overflow in unsafe Rust
/// This shows why Rust's safety guarantees matter

// No repr(C) - Rust chooses the layout!
#[derive(Debug)]
struct State {
    buffer: [u8; 5],
    secret: u8,
}

// With repr(C) for comparison
#[repr(C)]
#[derive(Debug)]
struct StateC {
    buffer: [u8; 5],
    secret: u8,
}

// Struct where Rust is MORE likely to reorder
#[derive(Debug)]
struct Reorderable {
    a: u8,       // 1 byte
    b: u64,      // 8 bytes (wants 8-byte alignment)
    c: u8,       // 1 byte
    d: u32,      // 4 bytes (wants 4-byte alignment)
}

#[repr(C)]
#[derive(Debug)]
struct ReorderableC {
    a: u8,
    b: u64,
    c: u8,
    d: u32,
}

impl State {
    pub fn new() -> Self {
        Self {
            buffer: [0u8; 5],
            secret: 42,
        }
    }
}

fn main() {
    let vec: Vec<u32> = vec![1,2,3];
    println!("{vec:?}");
    println!("=== Memory Layout Comparison ===\n");

    // Show sizes
    println!("State (no repr):     size={}, align={}",
        std::mem::size_of::<State>(), std::mem::align_of::<State>());
    println!("StateC (repr(C)):    size={}, align={}",
        std::mem::size_of::<StateC>(), std::mem::align_of::<StateC>());

    println!("\nReorderable (no repr):  size={}, align={}",
        std::mem::size_of::<Reorderable>(), std::mem::align_of::<Reorderable>());
    println!("ReorderableC (repr(C)): size={}, align={}",
        std::mem::size_of::<ReorderableC>(), std::mem::align_of::<ReorderableC>());

    // Show field offsets
    println!("\n=== Field Offsets ===\n");

    println!("Reorderable (Rust default layout):");
    println!("  offset of a (u8):  {}", std::mem::offset_of!(Reorderable, a));
    println!("  offset of b (u64): {}", std::mem::offset_of!(Reorderable, b));
    println!("  offset of c (u8):  {}", std::mem::offset_of!(Reorderable, c));
    println!("  offset of d (u32): {}", std::mem::offset_of!(Reorderable, d));

    println!("\nReorderableC (repr(C) - declaration order):");
    println!("  offset of a (u8):  {}", std::mem::offset_of!(ReorderableC, a));
    println!("  offset of b (u64): {}", std::mem::offset_of!(ReorderableC, b));
    println!("  offset of c (u8):  {}", std::mem::offset_of!(ReorderableC, c));
    println!("  offset of d (u32): {}", std::mem::offset_of!(ReorderableC, d));

    // Now show the overflow behavior difference
    println!("\n=== Buffer Overflow With vs Without repr(C) ===\n");

    let mut state = State::new();
    let mut state_c = StateC { buffer: [0u8; 5], secret: 42 };

    println!("Before overflow:");
    println!("  State:  buffer={:?}, secret={}", state.buffer, state.secret);
    println!("  StateC: buffer={:?}, secret={}", state_c.buffer, state_c.secret);

    unsafe {
        // Overflow both by writing 6 bytes
        let ptr = state.buffer.as_mut_ptr();
        let ptr_c = state_c.buffer.as_mut_ptr();

        for i in 0..6 {
            *ptr.add(i) = 0xFF;
            *ptr_c.add(i) = 0xFF;
        }
    }

    println!("\nAfter writing 6 bytes (1 byte overflow):");
    println!("  State:  buffer={:?}, secret={}", state.buffer, state.secret);
    println!("  StateC: buffer={:?}, secret={}", state_c.buffer, state_c.secret);

    println!("\nNote: Without repr(C), the overflow might corrupt something else,");
    println!("      or 'secret' might be placed BEFORE buffer in memory!");
}
