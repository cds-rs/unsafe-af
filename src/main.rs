const USE_ANSI: bool = false; // set true for terminal, false for playground

mod color {
    use super::USE_ANSI;

    pub fn red(byte: u8) -> String {
        if USE_ANSI {
            format!("\x1b[91m{byte:02x}\x1b[0m")
        } else {
            format!("[{byte:02x}]") // brackets for changed
        }
    }

    pub fn green(byte: u8) -> String {
        if USE_ANSI {
            format!("\x1b[92m{byte:02x}\x1b[0m")
        } else {
            format!("({byte:02x})") // parens for watched
        }
    }

    pub fn plain(byte: u8) -> String {
        format!(" {byte:02x} ")
    }
}

const VIEW_SIZE: usize = 16;
const BUFFER_SIZE: usize = 5;
const NUM_SIZE: usize = std::mem::size_of::<i32>();

struct MemoryView {
    snapshot: [u8; VIEW_SIZE],
    corrupted: [bool; VIEW_SIZE],
    num_offset: usize,
}

impl MemoryView {
    fn new(buffer_ptr: *const u8, num_ptr: *const i32) -> Self {
        let offset = (num_ptr as usize).wrapping_sub(buffer_ptr as usize);
        Self {
            snapshot: [0u8; VIEW_SIZE],
            corrupted: [false; VIEW_SIZE],
            num_offset: offset,
        }
    }

    fn capture(&mut self, ptr: *const u8) {
        unsafe {
            std::ptr::copy_nonoverlapping(ptr, self.snapshot.as_mut_ptr(), VIEW_SIZE);
        }
    }

    fn is_separator(&self, i: usize) -> bool {
        i == BUFFER_SIZE || i == self.num_offset
    }

    fn is_watched(&self, i: usize) -> bool {
        i >= self.num_offset && i < self.num_offset + NUM_SIZE
    }

    fn print_byte(&self, i: usize, byte: u8, changed: bool) {
        if self.is_separator(i) {
            print!(" |");
        }

        let formatted = if changed {
            color::red(byte)
        } else if self.is_watched(i) && !self.corrupted[i] {
            color::green(byte)
        } else {
            color::plain(byte)
        };
        print!("{formatted}");
    }

    fn print_row(&self, label: &str) {
        print!("{label} |");
        for (i, &byte) in self.snapshot.iter().enumerate() {
            self.print_byte(i, byte, false);
        }
        println!();
    }

    fn print_diff(&mut self, prev: &[u8; VIEW_SIZE], iter: usize) {
        print!("i={iter:<2} |");
        for (i, (&p, &c)) in prev.iter().zip(self.snapshot.iter()).enumerate() {
            self.print_byte(i, c, p != c);
        }
        println!();

        for i in 0..VIEW_SIZE {
            if prev[i] != self.snapshot[i] {
                self.corrupted[i] = true;
            }
        }
    }
}

fn test(end: usize) {
    let mut buffer: [u8; BUFFER_SIZE] = [0u8; BUFFER_SIZE];
    let num_above_buffer: i32 = 40_000;
    let ptr: *mut u8 = &raw mut buffer[0];
    let num_ptr: *const i32 = &raw const num_above_buffer;

    let mut view = MemoryView::new(ptr, num_ptr);
    view.capture(ptr);

    println!("\n=== 0..{end} === (num_above_buffer at offset {}) ===", view.num_offset);
    println!("     | buffer           | ...      | num_above_buffer (i32)");
    println!("-----+------------------+----------+------------------------");
    view.print_row("init");

    let mut prev_snapshot = view.snapshot;

    unsafe {
        for i in 0..end {
            *ptr.add(i) = i as u8;
            view.capture(ptr);
            view.print_diff(&prev_snapshot, i);
            prev_snapshot = view.snapshot;
        }
    }

    println!("\nbuffer: {:?}", buffer);
    println!("num_above_buffer: {} (0x{:08x})\n", num_above_buffer, num_above_buffer);
}

fn main() {
    for e in 6..=10 {
        test(e);
    }
}
