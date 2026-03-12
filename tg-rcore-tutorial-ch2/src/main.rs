//! Chapter 2 kernel: batch system with trap/syscall handling and tangram demo.

#![no_std]
#![no_main]
#![cfg_attr(target_arch = "riscv64", deny(warnings, missing_docs))]
#![cfg_attr(not(target_arch = "riscv64"), allow(dead_code))]

extern crate alloc;

use core::arch::asm;
use core::hint;
use core::mem::MaybeUninit;
use core::ptr::{addr_of_mut, read_volatile, NonNull};

#[macro_use]
extern crate tg_console;

use impls::{Console, SyscallContext};
use riscv::register::scause;
use tg_console::log;
use tg_kernel_context::LocalContext;
use tg_syscall::{Caller, SyscallId};
use virtio_drivers::device::gpu::VirtIOGpu;
use virtio_drivers::transport::mmio::{MmioTransport, VirtIOHeader};
use virtio_drivers::{BufferDirection, Hal, PhysAddr};

const TANGRAM_CASE_GROUP: &str = "ch2";
const TANGRAM_PIECES: usize = 7;
const FRAME_PAUSE_SPINS: usize = 8_000_000;
const FINAL_PAUSE_SPINS: usize = 180_000_000;

const DESIGN_W: usize = 1280;
const DESIGN_H: usize = 720;

const COLOR_BG: u32 = 0x00E4_E4_E4;

const HEAP_SIZE: usize = 16 * 1024 * 1024;
#[unsafe(link_section = ".bss.uninit")]
static mut HEAP_SPACE: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

#[global_allocator]
static HEAP_ALLOCATOR: linked_list_allocator::LockedHeap =
    linked_list_allocator::LockedHeap::empty();

#[repr(C, align(4096))]
struct PageAligned<const N: usize>([u8; N]);

const DMA_PAGES: usize = 2048;
const PAGE_SIZE: usize = 4096;
#[unsafe(link_section = ".bss.uninit")]
static mut DMA_SPACE: PageAligned<{ DMA_PAGES * PAGE_SIZE }> =
    PageAligned([0; DMA_PAGES * PAGE_SIZE]);
static mut DMA_NEXT_PAGE: usize = 0;

const VIRTIO_MMIO_START: usize = 0x1000_1000;
const VIRTIO_MMIO_STRIDE: usize = 0x1000;
const VIRTIO_MMIO_SLOTS: usize = 8;
const VIRTIO_MAGIC: u32 = 0x7472_6976;
const VIRTIO_DEVICE_ID_GPU: u32 = 16;

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

#[derive(Copy, Clone)]
struct TangramStage {
    o_visible_pieces: usize,
    s_visible_pieces: usize,
}

const TANGRAM_O: [PieceDef; TANGRAM_PIECES] = [
    tri(170, 110, 250, 110, 170, 190, 0x40, 0x40, 0xe0),
    quad(170, 190, 250, 110, 250, 380, 170, 460, 0x00, 0xd6, 0xff),
    tri(250, 110, 520, 110, 520, 380, 0xd8, 0x50, 0xe0),
    tri(430, 290, 520, 380, 520, 610, 0xe8, 0x40, 0x40),
    tri(170, 460, 340, 610, 170, 610, 0xf0, 0xb0, 0x20),
    quad(340, 460, 430, 550, 340, 640, 250, 550, 0x40, 0xe0, 0x40),
    tri(430, 520, 500, 610, 430, 610, 0xe8, 0x40, 0x40),
];

const TANGRAM_S: [PieceDef; TANGRAM_PIECES] = [
    tri(680, 280, 770, 190, 770, 370, 0xf0, 0xb0, 0x20),
    tri(770, 190, 860, 190, 860, 280, 0xe8, 0x40, 0x40),
    tri(860, 170, 1010, 170, 860, 280, 0xd8, 0x50, 0xe0),
    tri(860, 370, 1000, 460, 860, 550, 0xd8, 0x50, 0xe0),
    quad(770, 370, 860, 370, 860, 460, 770, 460, 0x40, 0xe0, 0x40),
    quad(620, 640, 800, 640, 860, 550, 680, 550, 0x00, 0x90, 0xff),
    tri(800, 640, 860, 550, 920, 550, 0xe8, 0x40, 0x40),
];

#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(include_str!(env!("APP_ASM")));

#[cfg(target_arch = "riscv64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn _start() -> ! {
    const STACK_SIZE: usize = 8 * 4096;
    #[unsafe(link_section = ".boot.stack")]
    static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

    core::arch::naked_asm!(
        "la sp, {stack} + {stack_size}",
        "j  {main}",
        stack = sym STACK,
        stack_size = const STACK_SIZE,
        main = sym rust_main,
    )
}

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

struct GpuDisplay {
    gpu: VirtIOGpu<VirtioHal, MmioTransport>,
    fb_ptr: *mut u8,
    fb_len: usize,
    width: usize,
    height: usize,
    stride_px: usize,
    scale_fp: usize,
    off_x: usize,
    off_y: usize,
}

impl GpuDisplay {
    fn init() -> Option<Self> {
        let header = find_gpu_mmio_header()?;
        let transport = unsafe { MmioTransport::new(header).ok()? };
        let mut gpu = VirtIOGpu::<VirtioHal, MmioTransport>::new(transport).ok()?;

        let (width, height) = gpu.resolution().ok()?;
        let width = width as usize;
        let height = height as usize;
        if width == 0 || height == 0 {
            return None;
        }

        let fb = gpu.setup_framebuffer().ok()?;
        if fb.is_empty() {
            return None;
        }

        let stride_px = fb.len() / 4 / height.max(1);
        if stride_px == 0 {
            return None;
        }

        let scale_fp = core::cmp::min((width * 1024) / DESIGN_W, (height * 1024) / DESIGN_H).max(1);
        let canvas_w = (DESIGN_W * scale_fp) / 1024;
        let canvas_h = (DESIGN_H * scale_fp) / 1024;
        let off_x = (width.saturating_sub(canvas_w)) / 2;
        let off_y = (height.saturating_sub(canvas_h)) / 2;

        let fb_ptr = fb.as_mut_ptr();
        let fb_len = fb.len();

        let mut display = Self {
            gpu,
            fb_ptr,
            fb_len,
            width,
            height,
            stride_px,
            scale_fp,
            off_x,
            off_y,
        };

        display.clear();
        if display.gpu.flush().is_err() {
            return None;
        }
        Some(display)
    }

    fn draw_stage(&mut self, stage: TangramStage) {
        self.clear();
        for piece in TANGRAM_O
            .iter()
            .take(stage.o_visible_pieces.min(TANGRAM_PIECES))
        {
            self.fill_piece(*piece);
        }
        for piece in TANGRAM_S
            .iter()
            .take(stage.s_visible_pieces.min(TANGRAM_PIECES))
        {
            self.fill_piece(*piece);
        }
        if self.gpu.flush().is_err() {
            log::warn!("gpu flush failed; keep console tangram output only");
        }
    }

    fn clear(&mut self) {
        let fb = self.framebuffer_mut();
        let b = (COLOR_BG & 0xff) as u8;
        let g = ((COLOR_BG >> 8) & 0xff) as u8;
        let r = ((COLOR_BG >> 16) & 0xff) as u8;
        for px in fb.chunks_exact_mut(4) {
            px[0] = b;
            px[1] = g;
            px[2] = r;
            px[3] = 0;
        }
    }

    fn tri_sign(px: isize, py: isize, ax: isize, ay: isize, bx: isize, by: isize) -> isize {
        (px - bx) * (ay - by) - (ax - bx) * (py - by)
    }

    fn fill_tri(&mut self, piece: PieceDef) {
        let scale_fp = self.scale_fp as isize;
        let off_x = self.off_x as isize;
        let off_y = self.off_y as isize;

        let x1 = off_x + piece.x1 * scale_fp / 1024;
        let y1 = off_y + piece.y1 * scale_fp / 1024;
        let x2 = off_x + piece.x2 * scale_fp / 1024;
        let y2 = off_y + piece.y2 * scale_fp / 1024;
        let x3 = off_x + piece.x3 * scale_fp / 1024;
        let y3 = off_y + piece.y3 * scale_fp / 1024;

        let min_x = core::cmp::max(0, core::cmp::min(x1, core::cmp::min(x2, x3))) as usize;
        let max_x = core::cmp::min(
            self.width as isize - 1,
            core::cmp::max(x1, core::cmp::max(x2, x3)),
        ) as usize;
        let min_y = core::cmp::max(0, core::cmp::min(y1, core::cmp::min(y2, y3))) as usize;
        let max_y = core::cmp::min(
            self.height as isize - 1,
            core::cmp::max(y1, core::cmp::max(y2, y3)),
        ) as usize;

        if min_x > max_x || min_y > max_y {
            return;
        }

        let b = piece.b;
        let g = piece.g;
        let r = piece.r;
        let stride_px = self.stride_px;
        let fb_len = self.fb_len;
        let fb = self.framebuffer_mut();

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as isize;
                let py = y as isize;
                let d1 = Self::tri_sign(px, py, x1, y1, x2, y2);
                let d2 = Self::tri_sign(px, py, x2, y2, x3, y3);
                let d3 = Self::tri_sign(px, py, x3, y3, x1, y1);
                let has_neg = d1 < 0 || d2 < 0 || d3 < 0;
                let has_pos = d1 > 0 || d2 > 0 || d3 > 0;

                if !(has_neg && has_pos) {
                    let off = (y * stride_px + x) * 4;
                    if off + 3 >= fb_len {
                        continue;
                    }
                    fb[off] = b;
                    fb[off + 1] = g;
                    fb[off + 2] = r;
                    fb[off + 3] = 0;
                }
            }
        }
    }

    fn fill_piece(&mut self, piece: PieceDef) {
        self.fill_tri(piece);
        if piece.n == 4 {
            let second = PieceDef {
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
            self.fill_tri(second);
        }
    }

    fn framebuffer_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.fb_ptr, self.fb_len) }
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

#[inline]
fn stage_for_app(app_index: usize) -> Option<TangramStage> {
    if app_index < TANGRAM_PIECES {
        Some(TangramStage {
            o_visible_pieces: app_index + 1,
            s_visible_pieces: 0,
        })
    } else if app_index < TANGRAM_PIECES * 2 {
        Some(TangramStage {
            o_visible_pieces: TANGRAM_PIECES,
            s_visible_pieces: app_index - TANGRAM_PIECES + 1,
        })
    } else {
        None
    }
}

/// Kernel entry point.
extern "C" fn rust_main() -> ! {
    unsafe { tg_linker::KernelLayout::locate().zero_bss() };
    init_heap();

    tg_console::init_console(&Console);
    tg_console::set_log_level(option_env!("LOG"));
    tg_console::test_log();

    tg_syscall::init_io(&SyscallContext);
    tg_syscall::init_process(&SyscallContext);

    let tangram_enabled = option_env!("TG_CH2_SELECTED_CASES") == Some(TANGRAM_CASE_GROUP);
    let mut gpu_display = if tangram_enabled {
        let display = GpuDisplay::init();
        if let Some(ref disp) = display {
            log::info!(
                "virtio-gpu framebuffer ready: {}x{}",
                disp.width,
                disp.height
            );
        } else {
            log::warn!("virtio-gpu framebuffer unavailable; fallback to console tangram output");
        }
        display
    } else {
        None
    };

    for (app_index, app) in tg_linker::AppMeta::locate().iter().enumerate() {
        let app_base = app.as_ptr() as usize;
        log::info!("load app{app_index} to {app_base:#x}");

        if tangram_enabled {
            if let Some(stage) = stage_for_app(app_index) {
                if let Some(display) = gpu_display.as_mut() {
                    display.draw_stage(stage);
                }
            }
        }

        run_user_app(app_index, app_base);

        println!();
        if tangram_enabled && stage_for_app(app_index).is_some() {
            pause(FRAME_PAUSE_SPINS);
        }
    }

    if tangram_enabled {
        if gpu_display.is_some() {
            println!("Tangram demo finished. Keeping last frame active...");
            loop {
                hint::spin_loop();
            }
        } else {
            println!("Tangram demo finished. Holding before shutdown...");
            pause(FINAL_PAUSE_SPINS);
        }
    }

    tg_sbi::shutdown(false)
}

fn run_user_app(app_index: usize, app_base: usize) {
    let mut ctx = LocalContext::user(app_base);

    let mut user_stack: MaybeUninit<[usize; 512]> = MaybeUninit::uninit();
    let user_stack_ptr = user_stack.as_mut_ptr() as *mut usize;
    *ctx.sp_mut() = unsafe { user_stack_ptr.add(512) } as usize;

    loop {
        unsafe { ctx.execute() };

        use scause::{Exception, Trap};
        match scause::read().cause() {
            Trap::Exception(Exception::UserEnvCall) => {
                use SyscallResult::*;
                match handle_syscall(&mut ctx) {
                    Done => continue,
                    Exit(code) => log::info!("app{app_index} exit with code {code}"),
                    Error(id) => {
                        log::error!("app{app_index} call an unsupported syscall {}", id.0)
                    }
                }
            }
            trap => log::error!("app{app_index} was killed because of {trap:?}"),
        }

        unsafe { asm!("fence.i") };
        break;
    }

    let _ = hint::black_box(&user_stack);
}

fn pause(spins: usize) {
    for _ in 0..spins {
        hint::spin_loop();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("{info}");
    tg_sbi::shutdown(true)
}

enum SyscallResult {
    Done,
    Exit(usize),
    Error(SyscallId),
}

fn handle_syscall(ctx: &mut LocalContext) -> SyscallResult {
    use tg_syscall::{SyscallId as Id, SyscallResult as Ret};

    let id = ctx.a(7).into();
    let args = [ctx.a(0), ctx.a(1), ctx.a(2), ctx.a(3), ctx.a(4), ctx.a(5)];

    match tg_syscall::handle(Caller { entity: 0, flow: 0 }, id, args) {
        Ret::Done(ret) => match id {
            Id::EXIT => SyscallResult::Exit(ctx.a(0)),
            _ => {
                *ctx.a_mut(0) = ret as usize;
                ctx.move_next();
                SyscallResult::Done
            }
        },
        Ret::Unsupported(id) => SyscallResult::Error(id),
    }
}

mod impls {
    use tg_syscall::{STDDEBUG, STDOUT};

    pub struct Console;

    impl tg_console::Console for Console {
        #[inline]
        fn put_char(&self, c: u8) {
            tg_sbi::console_putchar(c);
        }
    }

    pub struct SyscallContext;

    impl tg_syscall::IO for SyscallContext {
        fn write(
            &self,
            _caller: tg_syscall::Caller,
            fd: usize,
            buf: usize,
            count: usize,
        ) -> isize {
            match fd {
                STDOUT | STDDEBUG => {
                    print!("{}", unsafe {
                        core::str::from_utf8_unchecked(core::slice::from_raw_parts(
                            buf as *const u8,
                            count,
                        ))
                    });
                    count as isize
                }
                _ => {
                    tg_console::log::error!("unsupported fd: {fd}");
                    -1
                }
            }
        }
    }

    impl tg_syscall::Process for SyscallContext {
        #[inline]
        fn exit(&self, _caller: tg_syscall::Caller, _status: usize) -> isize {
            0
        }
    }
}

#[cfg(not(target_arch = "riscv64"))]
mod stub {
    #[unsafe(no_mangle)]
    pub extern "C" fn main() -> i32 {
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn __libc_start_main() -> i32 {
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn rust_eh_personality() {}
}