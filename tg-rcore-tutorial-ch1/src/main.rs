
#![no_std]
#![no_main]

// 【查询 SBI 显卡信息】这是 RISC-V 显卡标准调用
use core::arch::asm;
fn sbi_get_fb_addr() -> usize {
    let addr: usize;
    unsafe { asm!("ecall", in("a7") 0x42000, in("a6") 0x0, lateout("a0") addr); }
    addr
}

#[no_mangle]
fn main() -> ! {
    // 【关键！】打印显卡地址
    let fb_addr = sbi_get_fb_addr();
    loop {
        // 死循环打印，确保你能看到
        println!("FRAMEBUFFER = {:#x}", fb_addr);
    }
}


use core::ptr::write_volatile;

const WIDTH: usize = 1280;
const HEIGHT: usize = 720;
const FRAMEBUFFER: usize = 0x80200000; // 这是 QEMU RISC-V 显卡的默认地址

const COLOR_BLACK: u32   = 0x00000000;
const COLOR_RED: u32     = 0x00FF0000;
const COLOR_GREEN: u32   = 0x0000FF00;
const COLOR_BLUE: u32    = 0x0000FFFF;
const COLOR_YELLOW: u32  = 0x00FFFF00;
const COLOR_PURPLE: u32  = 0x00FF00FF;
const COLOR_CYAN: u32    = 0x0000FFFF;
const COLOR_ORANGE: u32  = 0x00FF8000;

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

fn draw_rect(x: i32, y: i32, w: i32, h: i32, color: u32) {
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px >= 0 && px < WIDTH as i32 && py >= 0 && py < HEIGHT as i32 {
                let off = (py as usize * WIDTH + px as usize) * 4;
                unsafe {
                    write_volatile((FRAMEBUFFER + off) as *mut u32, color);
                }
            }
        }
    }
}

fn clear_screen() {
    draw_rect(0, 0, WIDTH as i32, HEIGHT as i32, COLOR_BLACK);
}

fn draw_tangram(shape: &[[i32; 5]; 7]) {
    for piece in shape {
        let x = piece[0];
        let y = piece[1];
        let w = piece[2];
        let h = piece[3];
        let color = piece[4] as u32;
        draw_rect(x, y, w, h, color);
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
fn main() -> ! {
    clear_screen();
    draw_tangram(&TANGRAM_O);
    draw_tangram(&TANGRAM_S);
    loop {}
}

// //! # 第一章：应用程序与基本执行环境
// //!
// //! 本章实现了一个最简单的 RISC-V S 态裸机程序，展示操作系统的最小执行环境。
// //!
// //! ## 关键概念
// //!
// //! - `#![no_std]`：不使用 Rust 标准库，改用不依赖操作系统的核心库 `core`
// //! - `#![no_main]`：不使用标准的 `main` 入口，自定义裸函数 `_start` 作为入口
// //! - 裸函数（naked function）：不生成函数序言/尾声，可在无栈环境下执行
// //! - SBI（Supervisor Binary Interface）：S 态软件向 M 态固件请求服务的标准接口
// //!
// //! 教程阅读建议：
// //!
// //! - 先看 `_start`：理解无运行时情况下的最小启动流程；
// //! - 再看 `rust_main`：理解最小 I/O 路径（SBI 输出 + 关机）；
// //! - 最后看 `panic_handler`：理解 no_std 程序的异常收口方式。

// // 不使用标准库，因为裸机环境没有操作系统提供系统调用支持
// #![no_std]
// // 不使用标准入口，因为裸机环境没有 C runtime 进行初始化
// #![no_main]
// // RISC-V64 架构下启用严格警告和文档检查
// #![cfg_attr(target_arch = "riscv64", deny(warnings, missing_docs))]
// // 非 RISC-V64 架构允许死代码（用于 cargo publish --dry-run 在主机上通过编译）
// #![cfg_attr(not(target_arch = "riscv64"), allow(dead_code))]

// // 引入 SBI 调用库，提供 console_putchar（输出字符）和 shutdown（关机）功能
// // 启用 nobios 特性后，tg_sbi 内建了 M-mode 启动代码，无需外部 SBI 固件
// use tg_sbi::{console_putchar, shutdown};

// /// S 态程序入口点。
// ///
// /// 这是一个裸函数（naked function），放置在 `.text.entry` 段，
// /// 链接脚本将其安排在地址 `0x80200000`。
// ///
// /// 裸函数不生成函数序言和尾声，因此可以在没有栈的情况下执行。
// /// 它完成两件事：
// /// 1. 设置栈指针 `sp`，指向栈顶（栈从高地址向低地址增长）
// /// 2. 跳转到 Rust 主函数 `rust_main`
// #[cfg(target_arch = "riscv64")]
// #[unsafe(naked)]
// #[unsafe(no_mangle)]
// #[unsafe(link_section = ".text.entry")]
// unsafe extern "C" fn _start() -> ! {
//     // 栈大小：4 KiB
//     const STACK_SIZE: usize = 4096;

//     // 在 .bss.uninit 段中分配栈空间
//     #[unsafe(link_section = ".bss.uninit")]
//     static mut STACK: [u8; STACK_SIZE] = [0u8; STACK_SIZE];

//     core::arch::naked_asm!(
//         "la sp, {stack} + {stack_size}", // 将 sp 设置为栈顶地址
//         "j  {main}",                      // 跳转到 rust_main
//         stack_size = const STACK_SIZE,
//         stack      =   sym STACK,
//         main       =   sym rust_main,
//     )
// }

// /// S 态主函数：打印 "Hello, world!" 并关机。
// ///
// /// 通过 SBI 的 `console_putchar` 逐字节输出字符串，
// /// 然后调用 `shutdown` 正常关机退出 QEMU。
// extern "C" fn rust_main() -> ! {
//     for c in b"Hello, world!\n" {
//         console_putchar(*c);
//     }
//     shutdown(false) // false 表示正常关机
// }

// /// panic 处理函数。
// ///
// /// `#![no_std]` 环境下必须自行实现。发生 panic 时以异常状态关机。
// #[panic_handler]
// fn panic(_info: &core::panic::PanicInfo) -> ! {
//     shutdown(true) // true 表示异常关机
// }

// /// 非 RISC-V64 架构的占位模块。
// ///
// /// 提供 `main` 等符号，使得在主机平台（如 x86_64）上也能通过编译，
// /// 满足 `cargo publish --dry-run` 和 `cargo test` 的需求。
// #[cfg(not(target_arch = "riscv64"))]
// mod stub {
//     /// 主机平台占位入口
//     #[unsafe(no_mangle)]
//     pub extern "C" fn main() -> i32 {
//         0
//     }

//     /// C 运行时占位
//     #[unsafe(no_mangle)]
//     pub extern "C" fn __libc_start_main() -> i32 {
//         0
//     }

//     /// Rust 异常处理人格占位
//     #[unsafe(no_mangle)]
//     pub extern "C" fn rust_eh_personality() {}
// }
