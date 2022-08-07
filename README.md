# QOI on Rust with SIMD

An implementation of the Quite Ok Image Format created by Dominic Szablewski
This implementation aims to leverage the power of Same-Instruction-Multiple-Data (`SIMD`) operations for the fastest possible operation of the algorithm. 

[This code from the initial commits](https://github.com/AstroFloof/qoi-rust-simd/blob/98f0fde8d2568d46a5c6a86ae144d1b07206b789/src/qoi.rs#L82-L177)
takes about 90 ms vs the naive 580 ms to take the hashes of one of the first images from James Webb. 
That image weighs in at about 153.5 Megapixels, so not an insignificant amount by any means. 
It has an even greater effect on smaller images, too. 
For a 128x128 image the `SIMD` implementation takes 3-4 µs while the naive takes about 63 µs. 
This is more than a 22x speedup! 

Of course, that's just for the hashing. 
Currently the `SSSE3` encoding method is about 1.5x the speed of pure Rust, but the `SSSE3` decoding is 3.3x slower (WIP)

I am using this as a way to teach myself Rust, and apparently assembly too, since I was dissatisfied with the options I had with the current experimental `SIMD` API on the nightly branch.

## Why inline assembly, you `unsafe` fool?

The "intrinsics" from `std::arch:x86_64` are not actually intrinsics, because they are a callable function within the assembly shown by tools such as [`cargo-asm`](https://github.com/gnzlbg/cargo-asm) and [the Rust playground](https://play.rust-lang.org). I need to tightly control what happens, and if there are a bunch of calls happening, there's unhelpful overhead.

Supposedly higher-level APIs like `portable_simd` don't help much either. 
I assume it works much better with other tasks, but the nature of what I'm doing here makes this complicated to try to generate code for. 
Using these, my code was 3x slower than the pure rust approach, and produced 50KiB (text) of nonsense assembly trying to compensate for the lack of 8-bit multiplication and arrangement of data. 

My method, using inline assembly, just works. It is quite the pain in the ass to write, but the end result is worth it, in my opinion.

## Room for improvement?

- Support for other architectures
  - ~~Default to naive if target isn't one with something written specifically for it.~~
  - `AArch64` has 128-bit `SIMD` instructions (`NEON`), which include a lot of things that could be very useful!
  - `AVX`, `AVX2`, `AVX512` would make this even faster.
- Make this a proper library, potentially accessible through a FFI or as a handler - would be a useful tool in places where QOI already excels.
- The QOI specification is possibly too simple and limited. QOI 2, electric boogaloo anyone?
  - `RGB`/`RGBA` only
  - 8bpc only - no `HDR`

## Current things I can test
My home CPU is an Intel Xeon X5670, which is juiced to the teeth such that it can reach 4 GHz under single-threaded load. 
Since it is over 12 years old at this point, it only has up to `SSE4.2` plus some other things like `AES-NI` and `CLMUL` 
(which are vaguely useful but the early implementations of CLMUL were really slow).

I recently purchased a laptop for university with an Intel Core i5-11400H specifically to have `AVX`, `AVX2`, and `AVX-512`. 
Development of implementations using those instructions should happen soon.

I have access to `ARMv8` CPU with `NEON`, but not `SVE` or `SVE2` at the moment.
  
------------------------

**Since I only started Rust recently, please point out how bad I am!**

*Just do it nicely so I can improve :3*
  

