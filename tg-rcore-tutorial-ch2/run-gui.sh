#!/bin/bash

set -e

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$SCRIPT_DIR"

cargo build

qemu-system-riscv64 \
    -machine virt \
    -bios none \
    -device ramfb \
    -serial stdio \
    -display sdl \
    -kernel target/riscv64gc-unknown-none-elf/debug/tg-rcore-tutorial-ch2