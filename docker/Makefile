#
# Copyright 2023, Colias Group, LLC
#
# SPDX-License-Identifier: BSD-2-Clause
#

work_root := ..

id := rust-root-task-demo
image_tag := $(id)
container_name := $(id)
ARCH   ?= riscv64
ifeq ($(ARCH), riscv64)
TARGET := riscv64imac-unknown-none-elf
else ifeq($(ARCH), aarch64)
TARGET := aarch64-unknown-none
endif
uid := $(shell id -u)
gid := $(shell id -g)

.PHONY: none
none:

.PHONY: build
build:
	docker build \
		--build-arg UID=$(uid) --build-arg GID=$(gid) \
		--build-arg ARCH=$(ARCH) --build-arg TARGET=$(TARGET) \
		-t $(image_tag) .

.PHONY: run
run: build
	docker run -d --name $(container_name) \
		--mount type=bind,src=$(abspath $(work_root)),dst=/work \
		$(image_tag) sleep inf

.PHONY: exec
exec:
	docker exec -it $(container_name) bash

.PHONY: rm-container
rm-container:
	for id in $$(docker ps -aq -f "name=^$(container_name)$$"); do \
		docker rm -f $$id; \
	done

.PHONY: test
test: build
	docker run --rm \
		--mount type=bind,readonly,src=$(abspath $(work_root)),dst=/work \
		-i $(image_tag) \
		make test BUILD=/tmp/build
