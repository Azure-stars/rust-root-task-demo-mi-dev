# MicroKit Usage Guide.

Micro Intrudution:

https://trustworthy.systems/projects/microkit/

https://github.com/seL4/microkit/blob/main/docs/manual.md

The key of the microkit is PD, PD contains the cspace, vpsace, thread and context.

It allows you to use communication channel and share memory between pd.

- init, which is invoked by the system exactly once before any other of the PD's code;
- notified, which is invoked when another PD has performed a notify() operation on a CC connected to this PD;
- protected (optional), which is invoked when another PD of lower priority performs a ppcall() operation through a CC connected to this PD.

1. Microkit needs to define memory and channel manually in the system file. 
2. Run usertests needs to run init first, but how to combine linux app (based on libc) and microkit, run init function first and call user tests second.
3. How to solve page allocation and page mapping.
4. How to solve signal Handler.
