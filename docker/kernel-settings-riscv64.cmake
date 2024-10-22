#
# Copyright 2023, Colias Group, LLC
#
# SPDX-License-Identifier: BSD-2-Clause
#

# Basis for seL4 kernel configuration

set(KernelArch riscv CACHE STRING "")
set(KernelPlatform spike CACHE STRING "")
set(KernelSel4Arch riscv64 CACHE STRING "")
set(KernelVerificationBuild OFF CACHE BOOL "")
