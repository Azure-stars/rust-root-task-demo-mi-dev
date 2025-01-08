#
# Copyright 2023, Colias Group, LLC
#
# SPDX-License-Identifier: BSD-2-Clause
#

BUILD ?= build

build_dir := $(BUILD)

.PHONY: none
none:

.PHONY: clean
clean:
	rm -rf $(build_dir)

KERNEL ?= sel4

TARGET := aarch64-sel4

ifeq ($(KERNEL), sel4)
sel4_prefix := $(SEL4_INSTALL_DIR)
loader_artifacts_dir := $(SEL4_INSTALL_DIR)/bin
else ifeq ($(KERNEL), rel4)
sel4_prefix := $(REL4_INSTALL_DIR)
loader_artifacts_dir := $(REL4_INSTALL_DIR)/bin
endif

loader := $(loader_artifacts_dir)/sel4-kernel-loader
loader_cli := $(loader_artifacts_dir)/sel4-kernel-loader-add-payload

app_crate := root-task
app := $(build_dir)/$(app_crate).elf

qemu_args := 
qemu_args += -drive file=mount.img,if=none,format=raw,id=x0
qemu_args += -device virtio-blk-device,drive=x0

qemu_args += -netdev user,id=net0,hostfwd=tcp::6379-:6379
qemu_args += -device virtio-net-device,netdev=net0
qemu_args += -object filter-dump,id=net0,netdev=net0,file=packets.pcap

$(app): $(app).intermediate

# SEL4_TARGET_PREFIX is used by build.rs scripts of various rust-sel4 crates to locate seL4
# configuration and libsel4 headers.
.INTERMDIATE: $(app).intermediate
$(app).intermediate:
	SEL4_PREFIX=$(sel4_prefix) \
	RUSTFLAGS="-Clink-arg=-Tcrates/shim/link.ld" \
		cargo build \
			--target $(TARGET) \
			--target-dir $(abspath $(build_dir)/target) \
			--artifact-dir $(build_dir) \
			--release \
			-p shim -p test-thread
	SEL4_PREFIX=$(sel4_prefix) \
		cargo build \
			--target $(TARGET) \
			--target-dir $(abspath $(build_dir)/target) \
			--artifact-dir $(build_dir) \
			--release \
			-p blk-thread -p net-thread -p kernel-thread -p fs-thread
	cargo build \
		--target-dir $(build_dir)/target \
		--artifact-dir $(build_dir) \
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
	qemu-system-aarch64 \
		$(qemu_args) \
		-machine virt,virtualization=on -cpu cortex-a57 -m size=1G \
		-serial mon:stdio \
		-nographic \
		-kernel $(image)

.PHONY: run
run: $(image)
	$(qemu_cmd)
	rm $(image)

.PHONY: test
test: test.py $(image)
	python3 $< $(qemu_cmd)
