//! A parse frees everything it allocates. The grammar is rebuilt per parse, so a
//! reference cycle in it (chumsky `Recursive::declare`) leaks one grammar per
//! call: per script edit and per `^constant` use in the language server.
//! Its own test binary, since it swaps the global allocator to count live bytes.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering::Relaxed};

use ares_compiler::parser::{parse_script_file, parse_single_expression};

static LIVE: AtomicIsize = AtomicIsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size() as isize, Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size() as isize, Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        LIVE.fetch_add(n as isize - l.size() as isize, Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static A: Counting = Counting;

#[test]
fn parsing_frees_the_grammar() {
    let script = "[proc,p](int $a)(int)\nif (calc($a + 1) > 2) { $a = ~q(\"<$a>\"); }\nreturn($a);\n";
    let parse = || {
        drop(parse_script_file(script, "p.rs2").unwrap());
        drop(parse_single_expression("calc(1 + 2)", "c.constant", 0, 0).unwrap());
    };
    parse(); // first-use allocations that stay (lazy statics, thread locals)
    let before = LIVE.load(Relaxed);
    for _ in 0..100 {
        parse();
    }
    let grown = LIVE.load(Relaxed) - before;
    assert!(grown < 4096, "100 parses kept {grown} bytes");
}
