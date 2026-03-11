#![no_std]
#![no_main]

extern crate alloc;

use core::ptr::NonNull;
use core::ptr::addr_of_mut;
use core::ptr::read_volatile;
use tg_sbi::console_putchar;
use virtio_drivers::device::gpu::VirtIOGpu;
use virtio_drivers::transport::mmio::{MmioTransport, VirtIOHeader};
use virtio_drivers::{BufferDirection, Hal, PhysAddr};

#[global_allocator]
static HEAP_ALLOCATOR: linked_list_allocator::LockedHeap = linked_list_allocator::LockedHeap::empty();

const HEAP_SIZE: usize = 16 * 1024 * 1024;
#[unsafe(link_section = ".bss.uninit")]
static mut HEAP_SPACE: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

#[repr(C, align(4096))]
struct PageAligned<const N: usize>([u8; N]);

const DMA_PAGES: usize = 2048;
const PAGE_SIZE: usize = 4096;
#[unsafe(link_section = ".bss.uninit")]
static mut DMA_SPACE: PageAligned<{ DMA_PAGES * PAGE_SIZE }> = PageAligned([0; DMA_PAGES * PAGE_SIZE]);
static mut DMA_NEXT_PAGE: usize = 0;

const VIRTIO_MMIO_START: usize = 0x1000_1000;
const VIRTIO_MMIO_STRIDE: usize = 0x1000;
const VIRTIO_MMIO_SLOTS: usize = 8;
const VIRTIO_MAGIC: u32 = 0x7472_6976;
const VIRTIO_DEVICE_ID_GPU: u32 = 16;

const DESIGN_W: usize = 1280;
const DESIGN_H: usize = 720;

#[derive(Copy, Clone)]
struct PieceDef {
    n: u8,
    x1: isize,
    y1: isize,
    x2: isize,
    y2: isize,
    x3: isize,
    y3: isize,
    x4: isize,
    y4: isize,
    b: u8,
    g: u8,
    r: u8,
}

const fn tri(x1: isize, y1: isize, x2: isize, y2: isize, x3: isize, y3: isize, b: u8, g: u8, r: u8) -> PieceDef {
    PieceDef { n: 3, x1, y1, x2, y2, x3, y3, x4: 0, y4: 0, b, g, r }
}

const fn quad(
    x1: isize,
    y1: isize,
    x2: isize,
    y2: isize,
    x3: isize,
    y3: isize,
    x4: isize,
    y4: isize,
    b: u8,
    g: u8,
    r: u8,
) -> PieceDef {
    PieceDef { n: 4, x1, y1, x2, y2, x3, y3, x4, y4, b, g, r }
}

// Traditional tangram set for O: 2 large triangles, 1 medium triangle,
// 2 small triangles, 1 square, 1 parallelogram.
const TANGRAM_O: [PieceDef; 7] = [
    tri(170, 110, 250, 110, 170, 190, 0x40, 0x40, 0xe0), // small triangle (red)
    quad(170, 190, 250, 110, 250, 380, 170, 460, 0x00, 0xd6, 0xff), // parallelogram (yellow)
    tri(250, 110, 520, 110, 520, 380, 0xd8, 0x50, 0xe0), // large triangle (pink)
    tri(430, 290, 520, 380, 520, 610, 0xe8, 0x40, 0x40), // medium triangle (blue)
    tri(170, 460, 340, 610, 170, 610, 0xf0, 0xb0, 0x20), // large triangle (cyan)
    quad(340, 460, 430, 550, 340, 640, 250, 550, 0x40, 0xe0, 0x40), // square (green)
    tri(430, 520, 500, 610, 430, 610, 0xe8, 0x40, 0x40), // small triangle (blue)
];

// Traditional tangram set for S: 2 large triangles, 1 medium triangle,
// 2 small triangles, 1 square, 1 parallelogram.
const TANGRAM_S: [PieceDef; 7] = [
    tri(680, 280, 770, 190, 770, 370, 0xf0, 0xb0, 0x20), // large triangle (cyan)
    tri(770, 190, 860, 190, 860, 280, 0xe8, 0x40, 0x40), // small triangle (blue)
    tri(860, 170, 1010, 170, 860, 280, 0xd8, 0x50, 0xe0), // medium triangle (pink)
    tri(860, 370, 1000, 460, 860, 550, 0xd8, 0x50, 0xe0), // large triangle (pink)
    quad(770, 370, 860, 370, 860, 460, 770, 460, 0x40, 0xe0, 0x40), // square (green)
    quad(620, 640, 800, 640, 860, 550, 680, 550, 0x00, 0x90, 0xff), // parallelogram (orange)
    tri(800, 640, 860, 550, 920, 550, 0xe8, 0x40, 0x40), // small triangle (blue)
];

struct VirtioHal;

unsafe impl Hal for VirtioHal {
    fn dma_alloc(pages: usize, _direction: BufferDirection) -> (PhysAddr, NonNull<u8>) {
        unsafe {
            let start = DMA_NEXT_PAGE;
            let end = start + pages;
            if end > DMA_PAGES {
                panic!("dma alloc out of memory");
            }
            DMA_NEXT_PAGE = end;
            let base = addr_of_mut!(DMA_SPACE.0) as *mut u8 as usize;
            let paddr = base + start * PAGE_SIZE;
            let vaddr = NonNull::new(paddr as *mut u8).unwrap();
            (paddr, vaddr)
        }
    }

    unsafe fn dma_dealloc(_paddr: PhysAddr, _vaddr: NonNull<u8>, _pages: usize) -> i32 {
        0
    }

    unsafe fn mmio_phys_to_virt(paddr: PhysAddr, _size: usize) -> NonNull<u8> {
        NonNull::new(paddr as *mut u8).unwrap()
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direction: BufferDirection) -> PhysAddr {
        buffer.as_ptr() as *mut u8 as usize
    }

    unsafe fn unshare(_paddr: PhysAddr, _buffer: NonNull<[u8]>, _direction: BufferDirection) {}
}

#[cfg(target_arch = "riscv64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn _start() -> ! {
    const STACK_SIZE: usize = 8 * 4096;
    #[unsafe(link_section = ".bss.uninit")]
    static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

    core::arch::naked_asm!(
        "la sp, {stack} + {stack_size}",
        "j  {main}",
        stack = sym STACK,
        stack_size = const STACK_SIZE,
        main = sym rust_main,
    )
}

fn put_str(s: &str) {
    for b in s.as_bytes() {
        console_putchar(*b);
    }
}

fn put_dec(mut n: usize) {
    let mut buf = [0u8; 20];
    let mut i = 0;
    if n == 0 {
        console_putchar(b'0');
        return;
    }
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        console_putchar(buf[i]);
    }
}

fn init_heap() {
    unsafe {
        let heap_start = addr_of_mut!(HEAP_SPACE) as *mut u8;
        HEAP_ALLOCATOR.lock().init(heap_start, HEAP_SIZE);
    }
}

fn find_gpu_mmio_header() -> Option<NonNull<VirtIOHeader>> {
    for i in 0..VIRTIO_MMIO_SLOTS {
        let base = VIRTIO_MMIO_START + i * VIRTIO_MMIO_STRIDE;
        let magic = unsafe { read_volatile(base as *const u32) };
        let version = unsafe { read_volatile((base + 0x004) as *const u32) };
        let device_id = unsafe { read_volatile((base + 0x008) as *const u32) };

        if magic != VIRTIO_MAGIC {
            continue;
        }

        if device_id == 0 {
            continue;
        }

        if device_id == VIRTIO_DEVICE_ID_GPU && (version == 1 || version == 2) {
            return NonNull::new(base as *mut VirtIOHeader);
        }
    }
    None
}

fn tri_sign(px: isize, py: isize, ax: isize, ay: isize, bx: isize, by: isize) -> isize {
    (px - bx) * (ay - by) - (ax - bx) * (py - by)
}

fn fill_tri(
    fb: &mut [u8],
    stride_px: usize,
    width: usize,
    height: usize,
    piece: PieceDef,
    scale_fp: usize,
    off_x: usize,
    off_y: usize,
) {
    let x1 = off_x as isize + (piece.x1 * scale_fp as isize) / 1024;
    let y1 = off_y as isize + (piece.y1 * scale_fp as isize) / 1024;
    let x2 = off_x as isize + (piece.x2 * scale_fp as isize) / 1024;
    let y2 = off_y as isize + (piece.y2 * scale_fp as isize) / 1024;
    let x3 = off_x as isize + (piece.x3 * scale_fp as isize) / 1024;
    let y3 = off_y as isize + (piece.y3 * scale_fp as isize) / 1024;

    let min_x = core::cmp::max(0, core::cmp::min(x1, core::cmp::min(x2, x3))) as usize;
    let max_x = core::cmp::min(width as isize - 1, core::cmp::max(x1, core::cmp::max(x2, x3))) as usize;
    let min_y = core::cmp::max(0, core::cmp::min(y1, core::cmp::min(y2, y3))) as usize;
    let max_y = core::cmp::min(height as isize - 1, core::cmp::max(y1, core::cmp::max(y2, y3))) as usize;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as isize;
            let py = y as isize;
            let d1 = tri_sign(px, py, x1, y1, x2, y2);
            let d2 = tri_sign(px, py, x2, y2, x3, y3);
            let d3 = tri_sign(px, py, x3, y3, x1, y1);
            let has_neg = d1 < 0 || d2 < 0 || d3 < 0;
            let has_pos = d1 > 0 || d2 > 0 || d3 > 0;
            if !(has_neg && has_pos) {
                let off = (y * stride_px + x) * 4;
                if off + 3 >= fb.len() {
                    continue;
                }
                fb[off] = piece.b;
                fb[off + 1] = piece.g;
                fb[off + 2] = piece.r;
                fb[off + 3] = 0;
            }
        }
    }
}

fn fill_piece(
    fb: &mut [u8],
    stride_px: usize,
    width: usize,
    height: usize,
    piece: PieceDef,
    scale_fp: usize,
    off_x: usize,
    off_y: usize,
) {
    fill_tri(fb, stride_px, width, height, piece, scale_fp, off_x, off_y);
    if piece.n == 4 {
        let p2 = PieceDef {
            n: 3,
            x1: piece.x1,
            y1: piece.y1,
            x2: piece.x3,
            y2: piece.y3,
            x3: piece.x4,
            y3: piece.y4,
            x4: 0,
            y4: 0,
            b: piece.b,
            g: piece.g,
            r: piece.r,
        };
        fill_tri(fb, stride_px, width, height, p2, scale_fp, off_x, off_y);
    }
}

fn draw_tangram_os() -> Result<(), ()> {
    put_str("gpu:step1 mmio\n");
    let header = match find_gpu_mmio_header() {
        Some(h) => h,
        None => {
            put_str("gpu:fail mmio-scan\n");
            return Err(());
        }
    };
    let transport = match unsafe { MmioTransport::new(header) } {
        Ok(t) => t,
        Err(_) => {
            put_str("gpu:fail mmio\n");
            return Err(());
        }
    };

    put_str("gpu:step2 device\n");
    let mut gpu = match VirtIOGpu::<VirtioHal, MmioTransport>::new(transport) {
        Ok(g) => g,
        Err(_) => {
            put_str("gpu:fail device\n");
            return Err(());
        }
    };

    let (width, height) = match gpu.resolution() {
        Ok((w, h)) => (w as usize, h as usize),
        Err(_) => {
            put_str("gpu:fail resolution\n");
            return Err(());
        }
    };
    put_str("gpu:res=");
    put_dec(width);
    put_str("x");
    put_dec(height);
    put_str("\n");

    put_str("gpu:step3 framebuffer\n");
    let fb = match gpu.setup_framebuffer() {
        Ok(fb) => fb,
        Err(_) => {
            put_str("gpu:fail framebuffer\n");
            return Err(());
        }
    };

    put_str("gpu:fb_len=");
    put_dec(fb.len());
    put_str("\n");
    if fb.len() == 0 {
        put_str("gpu:fail fb_len_zero\n");
        return Err(());
    }

    let stride_px = fb.len() / 4 / height.max(1);
    if stride_px == 0 {
        put_str("gpu:fail stride\n");
        return Err(());
    }

    for px in fb.chunks_exact_mut(4) {
        px[0] = 0x18;
        px[1] = 0x14;
        px[2] = 0x14;
        px[3] = 0;
    }

    let scale_fp = core::cmp::min((width * 1024) / DESIGN_W, (height * 1024) / DESIGN_H).max(1);
    let canvas_w = (DESIGN_W * scale_fp) / 1024;
    let canvas_h = (DESIGN_H * scale_fp) / 1024;
    let off_x = (width.saturating_sub(canvas_w)) / 2;
    let off_y = (height.saturating_sub(canvas_h)) / 2;

    for piece in TANGRAM_O {
        fill_piece(fb, stride_px, width, height, piece, scale_fp, off_x, off_y);
    }
    for piece in TANGRAM_S {
        fill_piece(fb, stride_px, width, height, piece, scale_fp, off_x, off_y);
    }

    put_str("gpu:step4 flush\n");
    if gpu.flush().is_err() {
        put_str("gpu:fail flush\n");
        return Err(());
    }

    // Keep the GPU driver alive. Dropping it calls queue_unset(), which makes
    // QEMU display go inactive after a short moment.
    core::mem::forget(gpu);

    put_str("gpu:ok\n");
    Ok(())
}

extern "C" fn rust_main() -> ! {
    put_str("start\n");
    put_str("heap:init\n");
    init_heap();
    let _ = draw_tangram_os();
    put_str("enter:loop\n");

    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    put_str("panic\n");
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(target_arch = "riscv64"))]
#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    loop {}
}
