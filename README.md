# QOI on Rust with SIMD

An implementation of the [Quite Ok Image Format](https://qoiformat.org/), originally created by Dominic Szablewski
This implementation aims to leverage the power of Same-Instruction-Multiple-Data (SIMD) operations for the fastest possible operation of the algorithm. 

This code takes in raw RGBA data (in memory) and enodes it to a Quite Ok Image (also in memory). If the fact that it is solid memory to solid memory instead of some form of disk streaming is an issue, this might not be the library for you. Maybe at some point I could find a way to stream it to disk, but right now, big contiguous memory operations are optimal. 

I am using this as a way to teach myself Rust, and apparently assembly too, since I was dissatisfied with the options I had with the current experimental SIMD API on the nightly branch.

## Compiling

This library has a build.rs that gives you the ability to specify what level of SIMD you want. Most likely you just want to enable the `simd-max` feature in your `Cargo.toml`, along with specifying `target-cpu=native`. I have included more specific feature sets in there too. For example, if the machine you are compiling on has a differring feature set from your executing machine, you can specify what will work best for the executing machine. You can either use explicit features (`simd-x4`, `simd-m16`, etc.) or use `target-cpu` again.

## Speed

[This code from the initial commits](https://github.com/AstroFloof/qoi-rust-simd/blob/98f0fde8d2568d46a5c6a86ae144d1b07206b789/src/qoi.rs#L82-L177)
takes about 90 ms vs the naive 580 ms to completely hash one of the first images from James Webb. 
That image weighs in at about 153.5 Megapixels, so not an insignificant amount by any means. 
It has an even greater effect on smaller images, too. 
For a 128x128 image, the `SSSE3` implementation took 3-4 µs while the non-SIMD took closer to 63 µs. 
This is more than a 22x speedup! 

Of course, that's just for the hashing. Proper benchmarks will come eventually, but I can safely say that the working `SSSE3` method is much faster than the non-SIMD method.

## Why inline assembly, you `unsafe` fool?

The "intrinsics" from `std::arch` are not actually intrinsics, because they compile to callable functions, as shown by tools such as [`cargo-asm`](https://github.com/gnzlbg/cargo-asm), [the Rust playground](https://play.rust-lang.org), and [Godbolt Compiler Explorer](https://rust.godbolt.org/). Additionally, even if these intrinsics are inlined, they add unnecessary overhead by fetching from memory, doing one operation, and then putting it back without considering that further work can be done while it is already in the CPU register. 
Supposedly higher-level APIs like `portable_simd` do essentially the same thing, but with a layer of abstraction to enable platform-independent SIMD operations. 

I assume these methods work much better for other tasks, but the hashing function is best started with an integer fused multiply-add instruction, which is not easy to represent with simplistic `vec_sum = vec_1 + vec_2` -like abstractions as they stand today.
I did attempt to use these APIs in the very start of the project, but my code was *3x slower than the non-SIMD* implementation, and produced 50KiB of nonsense assembly trying to compensate for the lack of packed 8-bit multiplication in x86, and the order in which the data needed to be. 

My method - using inline assembly - just works. Eventually. It is quite difficult to write, but the end result is worth it, because the performance gains are quite significant over stock.

## Non SIMD optimizations

This project also includes some other architecture-specific operations that aren't exposed to the average programmer. 
This includes utilizing `x86` repeated string instructions. Decoding uses `rep scasb` for quickly determining the length of a `QOI_OP_RUN`, and `rep stosd` for splatting the same `RGBA` value for that count into the raw pixel output. Encoding uses the same principle with `rep scasd` and `rep stosb`.

It also includes some good old-fashioned bit manipulation wizardry, à la [Quake III Arena's Q_rsqrt](https://en.wikipedia.org/wiki/Fast_inverse_square_root#Overview_of_the_code), although less complicated and without imprecision. I use the single-pixel hashing function from [rapid-qoi](https://github.com/zakarumych/rapid-qoi), but I improved on it, making the resulting function use one less operation than the original. This is present in the platform-independent target as well.

Other, more conventional hacks are present here too, although the compiler took care of my idea to turn the decoding functions into a lookup table, because the match statement already did that apparently.

## Room for improvement?

- Support for other architectures
  - ~~Default to naive if target isn't one with something written specifically for it.~~
  - `AArch64` has 128-bit `SIMD` instructions (`NEON`), which include a lot of things that could be very useful!
  - `AVX`, `AVX2`, `AVX512` would make this even faster.
- Make this a proper library, potentially accessible through a FFI or as a handler - would be a useful tool in places where QOI already excels.
- The QOI specification is possibly too simple and limited. QOI 2, electric boogaloo anyone? Limitations include:
  - `RGB`/`RGBA` only
  - 8bpc only - no `HDR`

## Current things I can test
I recently purchased a laptop for university with an Intel Core i5-11400H specifically to have `AVX`, `AVX2`, and `AVX-512`. I'm not sure how I will be able to test `AVX-VNNI`, but I can probably ask a friend with an 12th generation Intel Core.
Development of implementations using those instructions should happen soon.

ARM testing, once I finish all x86, can be done on a VM, or by communicating with my phone. Either way, it gives me access to an AARCH64 CPU with `NEON` and hopefully `SVE` and `SVE2`.
  
------------------------

**Since I only started writing Rust recently, please feel free to take issue with the structure of my code, as well as some traps I may have sprung!**

*Just do it nicely so I can improve :3*
  

