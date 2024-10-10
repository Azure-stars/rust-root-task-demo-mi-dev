#
# Copyright 2023, Colias Group, LLC
#
# SPDX-License-Identifier: BSD-2-Clause
#

BUILD ?= build
ARCH  ?= aarch64
KERNEL?= sel4
DEBUG ?= false
qemu_args := 
ifeq ($(DEBUG), true)
qemu_args += -D qemu.log -d in_asm,int,pcall,cpu_reset,guest_errors
endif
ifeq ($(ARCH), riscv64)
TARGET := riscv64imac-sel4
qemu_args += -machine virt \
		 		-smp 2 -m 4096
else ifeq ($(ARCH), aarch64)
TARGET := aarch64-sel4
qemu_args += -machine virt,virtualization=on -cpu cortex-a57 \
		 		-smp 2 -m 1024
endif

build_dir := $(BUILD)

ifeq ($(KERNEL), sel4)
sel4_prefix := $(SEL4_INSTALL_DIR)
else ifeq ($(KERNEL), rel4)
sel4_prefix := /opt/reL4
endif

qemu_args += -drive file=mount.img,if=none,format=raw,id=x0
qemu_args += -device virtio-blk-device,drive=x0

qemu_args += -netdev user,id=net0,hostfwd=tcp::6379-:6379
qemu_args += -device virtio-net-device,netdev=net0
qemu_args += -object filter-dump,id=net0,netdev=net0,file=packets.pcap

# Kernel loader binary artifacts provided by Docker container:
# - `sel4-kernel-loader`: The loader binary, which expects to have a payload appended later via
#   binary patch.
# - `sel4-kernel-loader-add-payload`: CLI which appends a payload to the loader.
ifeq ($(LOCAL), true)
loader_artifacts_dir := ./bin
else
loader_artifacts_dir := /deps/bin
endif
loader := $(loader_artifacts_dir)/sel4-kernel-loader
loader_cli := $(loader_artifacts_dir)/sel4-kernel-loader-add-payload

.PHONY: none
none:

.PHONY: clean
clean:
	rm -rf $(build_dir)

app_crate := root-task
app := $(build_dir)/$(app_crate).elf
app_intermediate := $(build_dir)/$(app_crate).intermediate

$(app): $(app_intermediate)

# SEL4_TARGET_PREFIX is used by build.rs scripts of various rust-sel4 crates to locate seL4
# configuration and libsel4 headers.
.INTERMDIATE: $(app_intermediate)
$(app_intermediate):
	SEL4_PREFIX=$(sel4_prefix) \
	# RUSTFLAGS="-Clink-arg=-Tcrates/shim-comp/linker.ld" \
	# 	cargo build \
	# 		-Z build-std=core,alloc,compiler_builtins \
	# 		-Z build-std-features=compiler-builtins-mem \
	# 		--target $(TARGET) \
	# 		--target-dir $(abspath $(build_dir)/target) \
	# 		--out-dir $(build_dir) \
	# 		--release \
	# 		-p shim-comp
	SEL4_PREFIX=$(sel4_prefix) \
		cargo build \
			-Z build-std=core,alloc,compiler_builtins \
			-Z build-std-features=compiler-builtins-mem \
			--target $(TARGET) \
			--target-dir $(abspath $(build_dir)/target) \
			--out-dir $(build_dir) \
			--release \
			-p test-thread \
			-p kernel-thread -p blk-thread -p net-thread -p http-server
	SEL4_PREFIX=$(sel4_prefix) \
		cargo build \
			-Z build-std=core,alloc,compiler_builtins \
			-Z build-std-features=compiler-builtins-mem \
			--target $(TARGET) \
			--target-dir $(abspath $(build_dir)/target) \
			--out-dir $(build_dir) \
			--release \
			-p $(app_crate)
	

image := $(build_dir)/image.elf

# Append the payload to the loader using the loader CLI
$(image): $(app) $(loader) $(loader_cli)
	$(loader_cli) \
		--loader $(loader) \
		--sel4-prefix $(sel4_prefix) \
		--app $(app) \
		-o $@

qemu_cmd := \
	qemu-system-$(ARCH) \
		$(qemu_args) \
		-nographic -serial mon:stdio \
		-kernel $(image)

.PHONY: run
run: $(image)
	$(qemu_cmd)
	rm $(image)

debug: $(image)
	$(qemu_cmd) -s -S
	rm $(image)

fdt:
	@qemu-system-aarch64 -M 128m -machine virt,dumpdtb=virt.out
	fdtdump virt.out

.PHONY: test
test: test.py $(image)
	python3 $< $(qemu_cmd)
