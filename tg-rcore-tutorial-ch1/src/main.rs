#![no_std]
#![no_main]

use core::ptr::write_volatile;

// ==========================
// ✅ 【修复地址！】正确显存配置
// ==========================
const VIRTIO_GPU_FB_ADDR: usize = 0x10000000;  // 物理显存基址
const SCREEN_WIDTH:      usize = 800;          // 宽
const SCREEN_HEIGHT:     usize = 600;          // 高
const PIXEL_BYTES:       usize = 4;            // 每个像素 4 字节（必写）

// 颜色
const BLACK: u32 = 0xFF000000;
const RED:   u32 = 0xFFFF0000;
const BLUE:  u32 = 0xFF0000FF;
const GREEN: u32 = 0xFF00FF00;

// ==========================
// ✅ 【关键】显卡初始化（解决 display 未初始化）
// ==========================
fn gpu_init() {
    const GPU_REG_BASE: usize = 0x80001000;
    unsafe {
        write_volatile((GPU_REG_BASE + 0x10) as *mut u32, 0x100);
        write_volatile((GPU_REG_BASE + 0x1C) as *mut u32, SCREEN_WIDTH as u32);
        write_volatile((GPU_REG_BASE + 0x20) as *mut u32, SCREEN_HEIGHT as u32);
        write_volatile((GPU_REG_BASE + 0x14) as *mut u32, 0x101);
    }
}

// ==========================
// ✅ 【修复地址！】正确画像素函数
// 地址公式：基址 + (y * 宽 + x) * 4
// ==========================
fn draw_pixel(x: usize, y: usize, color: u32) {
    if x >= SCREEN_WIDTH || y >= SCREEN_HEIGHT {
        return;
    }

    // ✅ 正确地址计算（修复你之前的所有错误）
    let offset = (y * SCREEN_WIDTH + x) * PIXEL_BYTES;
    let address = VIRTIO_GPU_FB_ADDR + offset;

    unsafe {
        write_volatile(address as *mut u32, color);
    }
}

// 画矩形
fn fill_rect(x: usize, y: usize, w: usize, h: usize, c: u32) {
    for dy in 0..h {
        for dx in 0..w {
            draw_pixel(x + dx, y + dy, c);
        }
    }
}

// ==========================
// 七巧板 O + S
// ==========================
fn draw_o() {
    fill_rect(150, 100, 100, 120, RED);
    fill_rect(170, 120, 60, 80, BLACK);
}

fn draw_s() {
    fill_rect(450, 100, 100, 50, BLUE);
    fill_rect(450, 170, 100, 50, BLUE);
    fill_rect(450, 100, 40, 120, BLUE);
}

// ==========================
// 主程序
// ==========================
#[unsafe(no_mangle)]
fn main() -> ! {
    gpu_init(); // 先初始化显卡

    // 清屏
    for y in 0..SCREEN_HEIGHT {
        for x in 0..SCREEN_WIDTH {
            draw_pixel(x, y, BLACK);
        }
    }

    draw_o();
    draw_s();

    loop {}
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
