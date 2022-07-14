# QOI on Rust with SIMD

A future implementation of the Quite Ok Image Format created by Dominic Szablewski
This implementation aims to leverage the power of Same-Instruction-Multiple-Data (SIMD) operations for the fastest possible operation of the algorithm. [This code from the initial commits](https://github.com/AstroFloof/qoi-rust-simd/blob/98f0fde8d2568d46a5c6a86ae144d1b07206b789/src/qoi.rs#L82-L177) takes about 90 ms* to take the hashes of one of the first images from James Webb. That image weighs in at about 153.5 Megapixels, so not an insignificant amount by any means.

I am using this as a way to teach myself Rust, and apparently assembly too, since I was dissatisfied with the options I had with the current experimental SIMD API on the nightly branch. This project can only be used on the nightly branch of Rust, since it uses a whole whack of unstable features.

## Why inline assembly, you `unsafe` fool?

The "intrinsics" from `std::arch:x86_64` are not actually intrinsics, because they are a callable function within the assembly shown by tools such as [`cargo-asm`](https://github.com/gnzlbg/cargo-asm) and [the Rust playground](https://play.rust-lang.org). I need to tightly control what happens, and if there are a bunch of calls happening, there's unhelpful overhead.

Supposedly higher-level APIs like `portable_simd` don't help much either. I assume it works much better with other tasks, but the nature of what I'm doing here makes this complicated to try to generate code for. Using this, my code was 3x slower than the naive approach, and produced 50kb of nonsense assembly trying to compensate for the lack of 8-bit multiplication or explicit methods of loading from memory. 

My way is significantly faster (assuming the results are correct - tests needed) because the main chunk of it only takes 16 lines of assembly for 16 pixels (64 bytes).

## Room for improvement

Obviously I need to implement the rest of the QOI algorithm, but other nice things would be.
- Support for other architectures
  - Default to naive if target isn't one with something written specifically for it.
  - `ARMv8` (?) has 128-bit instructions (NEON). I have access to an `ARMv8` processor.
  - `AVX`, `AVX2`, `AVX512` would make this even faster, but I have no way to test that at present*.
- Make this a proper library, potentially accessible through a FFI or as a handler - would be a useful tool in places where QOI already excels.
- The QOI specification is possibly too simple and limited. QOI 2, electric boogaloo anyone?
  - RGB/RGBA only
  - Compression isn't great
  - 8bpc only - no HDR
  
-----------

**Since I only started Rust a week ago, please point out how bad I am**
Just do it nicely so I can improve :)
  
\*My CPU is an Intel Xeon X5670, which is juiced to the teeth such that it can reach 4 GHz under single-threaded load. Since it is over 12 years old at this point, it only has up to `SSE4.2` plus some other things like `AES-NI` and `CLMUL` (which might be useful, who knows?) I cannot test any features from AVX and onward.
