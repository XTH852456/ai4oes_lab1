#![no_std]
#![no_main]

use core::arch::{asm, naked_asm};
use core::ptr::write_volatile;
use tg_sbi::console_putchar;

const WIDTH: usize = 1280;
const HEIGHT: usize = 720;

const COLOR_BLACK: u32 = 0x00000000;
const COLOR_RED: u32 = 0x00FF0000;
const COLOR_GREEN: u32 = 0x0000FF00;
const COLOR_BLUE: u32 = 0x000000FF;
const COLOR_YELLOW: u32 = 0x00FFFF00;
const COLOR_PURPLE: u32 = 0x00FF00FF;
const COLOR_ORANGE: u32 = 0x00FF8000;

const TANGRAM_O: [[i32; 5]; 7] = [
    [200, 100, 200, 20, COLOR_RED as i32],
    [200, 280, 200, 20, COLOR_RED as i32],
    [190, 100, 20, 200, COLOR_GREEN as i32],
    [390, 100, 20, 200, COLOR_GREEN as i32],
    [220, 120, 160, 40, COLOR_BLUE as i32],
    [220, 220, 160, 40, COLOR_BLUE as i32],
    [280, 160, 40, 80, COLOR_YELLOW as i32],
];

const TANGRAM_S: [[i32; 5]; 7] = [
    [600, 100, 160, 20, COLOR_PURPLE as i32],
    [600, 180, 160, 20, COLOR_ORANGE as i32],
    [600, 260, 160, 20, COLOR_BLUE as i32],
    [590, 100, 20, 100, COLOR_RED as i32],
    [750, 180, 20, 100, COLOR_GREEN as i32],
    [620, 140, 60, 20, COLOR_YELLOW as i32],
    [690, 220, 60, 20, COLOR_ORANGE as i32],
];

#[cfg(target_arch = "riscv64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn _start() -> ! {
    const STACK_SIZE: usize = 4096;

    #[unsafe(link_section = ".bss.uninit")]
    static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

    naked_asm!(
        "la sp, {stack} + {stack_size}",
        "j  {main}",
        stack_size = const STACK_SIZE,
        stack = sym STACK,
        main = sym rust_main,
    )
}

#[inline]
fn sbi_get_fb_addr() -> (isize, usize) {
    let err: isize;
    let value: usize;
    unsafe {
        // Non-standard framebuffer SBI: return convention follows SbiRet { error, value }.
        asm!(
            "ecall",
            inlateout("a0") 0usize => err,
            inlateout("a1") 0usize => value,
            in("a7") 0x42000usize,
            in("a6") 0usize,
        );
    }
    (err, value)
}

fn put_byte(b: u8) {
    console_putchar(b);
}

fn put_str(s: &str) {
    for b in s.as_bytes() {
        put_byte(*b);
    }
}

fn put_hex_usize(v: usize) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    put_str("0x");
    for i in (0..(core::mem::size_of::<usize>() * 2)).rev() {
        let nibble = (v >> (i * 4)) & 0xF;
        put_byte(HEX[nibble]);
    }
}

fn draw_rect(fb_addr: usize, x: i32, y: i32, w: i32, h: i32, color: u32) {
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px >= 0 && px < WIDTH as i32 && py >= 0 && py < HEIGHT as i32 {
                let off = (py as usize * WIDTH + px as usize) * 4;
                unsafe {
                    write_volatile((fb_addr + off) as *mut u32, color);
                }
            }
        }
    }
}

fn clear_screen(fb_addr: usize) {
    draw_rect(fb_addr, 0, 0, WIDTH as i32, HEIGHT as i32, COLOR_BLACK);
}

fn draw_tangram(fb_addr: usize, shape: &[[i32; 5]; 7]) {
    for piece in shape {
        draw_rect(fb_addr, piece[0], piece[1], piece[2], piece[3], piece[4] as u32);
    }
}

extern "C" fn rust_main() -> ! {
    let (fb_err, fb_addr) = sbi_get_fb_addr();
    put_str("FB_ECODE = ");
    put_hex_usize(fb_err as usize);
    put_str("\n");
    put_str("FRAMEBUFFER = ");
    put_hex_usize(fb_addr);
    put_str("\n");

    if fb_err < 0 || fb_addr == 0 {
        put_str("framebuffer not available\n");
        loop {}
    }

    clear_screen(fb_addr);
    draw_tangram(fb_addr, &TANGRAM_O);
    draw_tangram(fb_addr, &TANGRAM_S);
    loop {}
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
