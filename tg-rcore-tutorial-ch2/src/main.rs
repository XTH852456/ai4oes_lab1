//! Chapter 2 kernel: batch system with trap/syscall handling and tangram demo.

#![no_std]
#![no_main]
#![cfg_attr(target_arch = "riscv64", deny(warnings, missing_docs))]
#![cfg_attr(not(target_arch = "riscv64"), allow(dead_code))]

use core::arch::asm;
use core::hint;
use core::mem::MaybeUninit;
use core::ptr::write_volatile;

#[macro_use]
extern crate tg_console;

use impls::{Console, SyscallContext};
use riscv::register::scause;
use tg_console::log;
use tg_kernel_context::LocalContext;
use tg_syscall::{Caller, SyscallId};

const TANGRAM_CASE_GROUP: &str = "ch2";
const TANGRAM_PIECES: usize = 7;
const FRAME_PAUSE_SPINS: usize = 30_000_000;
const FINAL_PAUSE_SPINS: usize = 180_000_000;

const FB_WIDTH: usize = 1280;
const FB_HEIGHT: usize = 720;

const COLOR_BG: u32 = 0x0014_1418;
const COLOR_RED: u32 = 0x00D6_4B4B;
const COLOR_BLUE: u32 = 0x0048_88D9;
const COLOR_GREEN: u32 = 0x004E_BF85;
const COLOR_GOLD: u32 = 0x00E2_B714;
const COLOR_ORANGE: u32 = 0x00E2_8B38;
const COLOR_CYAN: u32 = 0x003A_BDD6;
const COLOR_PINK: u32 = 0x00D9_6AB3;

#[derive(Copy, Clone)]
enum Shape {
	O,
	S,
}

#[derive(Copy, Clone)]
struct Rect {
	x: i32,
	y: i32,
	w: i32,
	h: i32,
	color: u32,
}

impl Rect {
	const fn new(x: i32, y: i32, w: i32, h: i32, color: u32) -> Self {
		Self { x, y, w, h, color }
	}
}

#[derive(Copy, Clone)]
struct TangramStage {
	shape: Shape,
	visible_pieces: usize,
}

const TANGRAM_O: [Rect; TANGRAM_PIECES] = [
	Rect::new(180, 120, 220, 28, COLOR_RED),
	Rect::new(180, 332, 220, 28, COLOR_BLUE),
	Rect::new(166, 120, 28, 240, COLOR_GREEN),
	Rect::new(392, 120, 28, 240, COLOR_GOLD),
	Rect::new(208, 152, 168, 44, COLOR_ORANGE),
	Rect::new(208, 260, 168, 44, COLOR_CYAN),
	Rect::new(268, 196, 52, 108, COLOR_PINK),
];

const TANGRAM_S: [Rect; TANGRAM_PIECES] = [
	Rect::new(650, 120, 180, 28, COLOR_RED),
	Rect::new(650, 220, 180, 28, COLOR_ORANGE),
	Rect::new(650, 320, 180, 28, COLOR_BLUE),
	Rect::new(636, 120, 28, 128, COLOR_GREEN),
	Rect::new(816, 220, 28, 128, COLOR_GOLD),
	Rect::new(676, 168, 74, 32, COLOR_CYAN),
	Rect::new(736, 268, 74, 32, COLOR_PINK),
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

#[derive(Copy, Clone)]
struct FrameBuffer {
	addr: usize,
}

impl FrameBuffer {
	fn probe() -> Option<Self> {
		let (err, addr) = sbi_get_fb_addr();
		if err < 0 || addr == 0 {
			None
		} else {
			Some(Self { addr })
		}
	}

	fn clear(&self) {
		self.draw_rect(Rect::new(0, 0, FB_WIDTH as i32, FB_HEIGHT as i32, COLOR_BG));
	}

	fn draw_stage(&self, stage: TangramStage) {
		self.clear();
		let pieces = match stage.shape {
			Shape::O => &TANGRAM_O,
			Shape::S => &TANGRAM_S,
		};
		for rect in pieces.iter().take(stage.visible_pieces.min(pieces.len())) {
			self.draw_rect(*rect);
		}
	}

	fn draw_rect(&self, rect: Rect) {
		for dy in 0..rect.h {
			for dx in 0..rect.w {
				let px = rect.x + dx;
				let py = rect.y + dy;
				if px < 0 || py < 0 || px >= FB_WIDTH as i32 || py >= FB_HEIGHT as i32 {
					continue;
				}
				let offset = (py as usize * FB_WIDTH + px as usize) * 4;
				unsafe {
					write_volatile((self.addr + offset) as *mut u32, rect.color);
				}
			}
		}
	}
}

#[inline]
fn stage_for_app(app_index: usize) -> Option<TangramStage> {
	if app_index < TANGRAM_PIECES {
		Some(TangramStage {
			shape: Shape::O,
			visible_pieces: app_index + 1,
		})
	} else if app_index < TANGRAM_PIECES * 2 {
		Some(TangramStage {
			shape: Shape::S,
			visible_pieces: app_index - TANGRAM_PIECES + 1,
		})
	} else {
		None
	}
}

#[inline]
fn sbi_get_fb_addr() -> (isize, usize) {
	let err: isize;
	let value: usize;
	unsafe {
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

/// Kernel entry point.
extern "C" fn rust_main() -> ! {
	unsafe { tg_linker::KernelLayout::locate().zero_bss() };

	tg_console::init_console(&Console);
	tg_console::set_log_level(option_env!("LOG"));
	tg_console::test_log();

	tg_syscall::init_io(&SyscallContext);
	tg_syscall::init_process(&SyscallContext);

	let tangram_enabled = option_env!("TG_CH2_SELECTED_CASES") == Some(TANGRAM_CASE_GROUP);
	let framebuffer = if tangram_enabled {
		match FrameBuffer::probe() {
			Some(fb) => {
				log::info!("framebuffer detected at {:#x}", fb.addr);
				Some(fb)
			}
			None => {
				log::warn!("framebuffer unavailable; fallback to console tangram output");
				None
			}
		}
	} else {
		None
	};

	for (app_index, app) in tg_linker::AppMeta::locate().iter().enumerate() {
		let app_base = app.as_ptr() as usize;
		log::info!("load app{app_index} to {app_base:#x}");

		if tangram_enabled {
			if let Some(stage) = stage_for_app(app_index) {
				if let Some(fb) = framebuffer {
					fb.draw_stage(stage);
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
		println!("Tangram demo finished. Holding final frame before shutdown...");
		pause(FINAL_PAUSE_SPINS);
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
					Error(id) => log::error!("app{app_index} call an unsupported syscall {}", id.0),
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
