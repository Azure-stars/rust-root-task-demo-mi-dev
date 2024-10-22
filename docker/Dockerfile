#
# Copyright 2023, Colias Group, LLC
#
# SPDX-License-Identifier: BSD-2-Clause
#

FROM trustworthysystems/sel4

RUN apt-get update -q && apt-get install -y --no-install-recommends \
    wget \
    sudo man vim \
    bash-completion \
    && rm -rf /var/lib/apt/lists/*

# RUN curl -sSf https://sh.rustup.rs | \
#         bash -s -- -y --no-modify-path --default-toolchain none
RUN curl -sSf https://sh.rustup.rs | \
        bash -s -- -y --no-modify-path \
            --default-toolchain nightly-2024-09-01 \
            --component rust-src

ENV PATH=/root/.cargo/bin:$PATH

RUN mkdir /deps

WORKDIR /deps

RUN set -eux; \
    git clone \
        https://github.com/seL4/seL4.git \
        --config advice.detachedHead=false; \
    cd seL4; \
    git checkout cd6d3b8c25d49be2b100b0608cf0613483a6fffa;

RUN apt-get update -q && apt-get install -y --no-install-recommends gcc-riscv64-linux-gnu && rm -rf /var/lib/apt/lists/*

ENV SEL4_INSTALL_DIR=/deps/seL4/install

ARG ARCH=aarch64
ARG TARGET=aarch64-unknown-none

COPY kernel-settings-${ARCH}.cmake ./kernel-settings.cmake
RUN set -eux; \
    cd seL4; \
    cmake \
        -DCROSS_COMPILER_PREFIX=${ARCH}-linux-gnu- \
        -DCMAKE_INSTALL_PREFIX=install \
        -C ../kernel-settings.cmake \
        -G Ninja \
        -S . \
        -B build; \
    ninja -C build all; \
    ninja -C build install;

# RUN set -eux; \
#     url="https://github.com/rel4team/rust-sel4"; \
#     branch="mi_dev"; \
#     remote_options="--git $url --branch $branch"; \
#     CC=${ARCH}-linux-gnu-gcc \
#     SEL4_PREFIX=$SEL4_INSTALL_DIR \
#         cargo install \
#             -Z build-std=core,alloc,compiler_builtins \
#             -Z build-std-features=compiler-builtins-mem \
#             --target ${TARGET} \
#             --root . \
#             $remote_options \
#             sel4-kernel-loader; \
#     cargo install \
#         --root . \
#         $remote_options \
#         sel4-kernel-loader-add-payload;

RUN set -eux; \
    url="https://github.com/seL4/rust-sel4"; \
    rev="1cd063a0f69b2d2045bfa224a36c9341619f0e9b"; \
    common_args="--git $url --rev $rev --root $SEL4_INSTALL_DIR"; \
    CC_aarch64_unknown_none=aarch64-linux-gnu-gcc \
    SEL4_PREFIX=$SEL4_INSTALL_DIR \
        cargo install \
            -Z build-std=core,compiler_builtins \
            -Z build-std-features=compiler-builtins-mem \
            --target aarch64-unknown-none \
            $common_args \
            sel4-kernel-loader; \
    cargo install \
        $common_args \
        sel4-kernel-loader-add-payload;

# Clone rel4 kernel
RUN set -eux; \
    git clone \
        https://github.com/rel4team/rel4_kernel.git -b mi_dev \
        --config advice.detachedHead=false;
RUN set -eux; \
    git clone \
        https://github.com/rel4team/seL4_c_impl.git \
        --config advice.detachedHead=false -b mi_dev;

# Build rel4 kernel. Just for riscv64 temporarily
COPY kernel-settings-riscv64.cmake .
RUN set -eux; \
    cd rel4_kernel;\
    rustup target add riscv64imac-unknown-none-elf; \
    make run;\
    cd ../seL4_c_impl; \
    cmake \
        -DCROSS_COMPILER_PREFIX=riscv64-linux-gnu- \
        -DCMAKE_INSTALL_PREFIX=/opt/reL4 \
        -DKernelPlatform=spike \
        -C ../kernel-settings-riscv64.cmake \
        -G Ninja \
        -S . \
        -B build; \
    ninja -C build all; \
    ninja -C build install; \
    rm -rf $(pwd);

ARG UID
ARG GID

RUN groupadd -f -g $GID x && useradd -u $UID -g $GID -G sudo -m -p x x
RUN echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers # for convenience

USER x

# This time, for the non-root user
RUN curl -sSf https://sh.rustup.rs | \
        bash -s -- -y --no-modify-path --default-toolchain none

ENV PATH=/home/x/.cargo/bin:$PATH
ENV ARCH=${ARCH}

WORKDIR /work
