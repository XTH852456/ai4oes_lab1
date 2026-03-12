#!/bin/bash

set -e

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$SCRIPT_DIR"

cargo build

qemu-system-riscv64 \
    -machine virt \
    -bios none \
    -monitor none \
    -serial stdio \
    -device virtio-gpu-device,xres=1280,yres=720,max_outputs=1 \
    -display gtk,gl=off \
    -kernel target/riscv64gc-unknown-none-elf/debug/tg-rcore-tutorial-ch2