# QOI on Rust with SIMD

An implementation of the [Quite Ok Image Format](https://qoiformat.org/), originally created by Dominic Szablewski
This implementation aims to leverage the power of Same-Instruction-Multiple-Data (SIMD) operations for the fastest possible operation of the algorithm. 

This library takes in raw RGBA data (in memory) and encodes it to a Quite Ok Image (also in memory), as well as the reverse of that. 
If the fact that it is solid memory to solid memory instead of some form of disk streaming is an issue, this might not be the library for you. 
Maybe at some point I could find a way to stream it to disk, but right now, big contiguous memory operations are optimal. 

I am using this as a way to teach myself Rust, and apparently assembly too.
## Compiling

Specify `target-cpu=native` to make use of the highest level of optimization with the lease amount of compilation. 

## Speed

Current benchmarks show that the x86_64v2 (makes use of the first level of optimization) is faster than x86_64v1 by a significant margin,
but the levels above that are no faster. This is a work in progress, and my working hypothesis is that I'm actually limited
by my RAM speed instead of anything on the CPU.

## Why inline assembly, you `unsafe` fool?

The "intrinsics" from `core::arch` are largely unstable still, so to make this compatible with stable rust I've used assembly instead of intrinsics. 
It allows more control over exactly what's going on under the hood that is unparalleled. 
Additionally, when respective target features are added they act as callables. This adds unnecessary overhead by fetching from memory, 
doing one operation, and then putting it back into memory without for each call.

Supposedly higher-level APIs like `portable_simd` do essentially the same thing, 
but with a layer of abstraction to enable platform-independent SIMD operations. While technically this is a good thing, 
it is misleading as some operations simply don't exist. For example, I tried to do a multiplication of two u8x16 vectors and
ended up with an output function a mile long and turtle slow. This is because there is no such instruction, and it was
subsequently trying to reinvent multiplication. 

I assume these methods work much better for other tasks, but the hashing function is best started with an 
integer fused multiply-add instruction, which is not easy to represent with simplistic abstractions such as `vec_sum = vec_1 + vec_2`.

Inline assembly just works... eventually. It is quite difficult to write, but the end result is worth it.

## Non SIMD optimizations

This project also includes some other architecture-specific operations that aren't exposed to the average programmer. 
This includes utilizing `x86` repeated string instructions. Decoding uses `rep scasb` for quickly determining the length of a `QOI_OP_RUN`, and `rep stosd` for splatting the same `RGBA` value for that count into the raw pixel output. Encoding uses the same principle with `rep scasd` and `rep stosb`.

It also includes some good old-fashioned bit manipulation wizardry, à la [Quake III Arena's Q_rsqrt](https://en.wikipedia.org/wiki/Fast_inverse_square_root#Overview_of_the_code), although less complicated and without imprecision. I use the single-pixel hashing function from [rapid-qoi](https://github.com/zakarumych/rapid-qoi), but I improved on it, making the resulting function use one less operation than the original. This is present in the platform-independent target as well.

Other, more conventional hacks are present here too, although the compiler took care of my idea to turn the decoding functions into a lookup table, because the match statement already did that, apparently.

## Room for improvement?

- Support for other architectures
  - ~~Default to naive if target isn't one with something written specifically for it.~~
  - `AArch64` has 128-bit `SIMD` instructions (`NEON`), which include a lot of things that could be very useful!
  - ~~`AVX`, `AVX2`, `AVX512` would make this even faster.~~ or not. investigation needed.
- Make this a proper library, potentially accessible through an FFI or as a handler - would be a useful tool in places where QOI already excels.
- The QOI specification is possibly too simple and limited. QOI 2, electric boogaloo anyone? Limitations include:
  - `RGB`/`RGBA` only
  - 8bpc only - no `HDR`

## Current things I can test
I recently purchased a laptop for university with an Intel Core i5-11400H specifically to have `AVX`, `AVX2`, and `AVX-512`. I'm not sure how I will be able to test `AVX-VNNI`, but I can probably ask a friend with a 12th generation Intel Core.
Development of implementations using those instructions should happen soon.

ARM testing, once I finish all x86, can be done on a VM, or by communicating with my phone. Either way, it gives me access to an AARCH64 CPU with `NEON` and hopefully `SVE` and `SVE2`.
  
------------------------

**Since I only started writing Rust recently, please feel free to take issue with the structure of my code, as well as some traps I may have sprung!**

*Just do it nicely so I can improve :3*
  

