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
            --default-toolchain nightly-2024-05-01 \
            --component rust-src

ENV PATH=/root/.cargo/bin:$PATH

RUN mkdir /deps

WORKDIR /deps

RUN set -eux; \
    git clone \
        https://github.com/seL4/seL4.git \
        --config advice.detachedHead=false; \
    cd seL4; \
    git checkout 1c7a0cb549021bc0781b49aa69359ee8d035981c;

RUN apt-get update -q && apt-get install -y --no-install-recommends gcc-riscv64-linux-gnu && rm -rf /var/lib/apt/lists/*

ENV SEL4_INSTALL_DIR=/deps/seL4/install

ARG ARCH=riscv64
ARG TARGET=riscv64imac-unknown-none-elf

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

RUN set -eux; \
    export RUSTUP_TOOLCHAIN=nightly-2024-02-01; \
    rustup component add rust-src; \
    url="https://github.com/seL4/rust-sel4"; \
    rev="cbfc9fffd4f9b9163fed42ac824ee4086c80d40f"; \
    remote_options="--git $url --rev $rev"; \
    CC=${ARCH}-linux-gnu-gcc \
    SEL4_PREFIX=$SEL4_INSTALL_DIR \
        cargo install \
            -Z build-std=core,alloc,compiler_builtins \
            -Z build-std-features=compiler-builtins-mem \
            --target ${TARGET} \
            --root . \
            $remote_options \
            sel4-kernel-loader; \
    cargo install \
        --root . \
        $remote_options \
        sel4-kernel-loader-add-payload;


# Clone rel4 kernel
COPY lib_patch.patch .
RUN set -eux; \
    git clone \
        https://github.com/rel4team/rel4_kernel.git \
        --config advice.detachedHead=false; \
    cd rel4_kernel; \
    git checkout cf2cbf43ff6bbebd2a4390ec1ca9c1fa98085ba9; \
    git apply ../lib_patch.patch;
RUN set -eux; \
    git clone \
        https://github.com/rel4team/seL4_c_impl.git \
        --config advice.detachedHead=false; \
    cd seL4_c_impl; \
    git checkout 552f173d3d7780b33184ebedefc58329ea5de3ba;

# Build rel4 kernel
RUN set -eux; \
    cd rel4_kernel;\
    rustup target add riscv64imac-unknown-none-elf; \
    make run;\
    cd ../seL4_c_impl; \
    cmake \
        -DCROSS_COMPILER_PREFIX=riscv64-linux-gnu- \
        -DCMAKE_INSTALL_PREFIX=/opt/reL4 \
        -DKernelPlatform=spike \
        -C ../kernel-settings.cmake \
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
