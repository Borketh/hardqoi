use alloc::vec::Vec;
use core::arch::x86_64::*;

use hardqoi::common::{HASH, QOI_OP_RUN, RGBA};

pub(crate) type Zmm = __m512i;
pub(crate) type Xmm = __m128i;

#[inline]
pub const fn is_alpha_different<const IMAGE_HAS_ALPHA: bool>(a: u32, b: u32) -> bool {
    IMAGE_HAS_ALPHA && (a & 0xff000000) != (b & 0xff000000)
    // (a >> 24) as u8 != (b >> 24) as u8
}

#[inline]
pub const fn hash_rgba(pixel: RGBA) -> HASH {
    let pixel = pixel as u64;

    // the first two lines do the same as rapid-qoi
    let duplicated = pixel.wrapping_mul(0x0000000100000001_u64);
    let a0g00b0r = duplicated & 0xff00ff0000ff00ff_u64;
    // this magic number puts the hash in the top 6 bits instead of the top 8
    let hash_high6 = a0g00b0r.wrapping_mul(0x0c001c000014002c_u64);
    let hash = hash_high6 >> 58; // now there's no need for the last mask

    hash as HASH
}

#[inline(always)]
/// # Safety
/// Assumes the length of the output bytes matches where the last bytes were written.
/// Handles its own allocation.
pub unsafe fn maybe_write_run(
    output_bytes: &mut Vec<u8>,
    maybe_run_length: Option<usize>,
) -> *mut u8 {
    if let Some(run_length) = maybe_run_length {
        let full_runs = run_length / 62;
        output_bytes.reserve(full_runs + 1);
        let extra_len = write_run(output_bytes.get_write_head(), full_runs, run_length % 62);
        output_bytes.add_len(extra_len);
    }
    output_bytes.get_write_head()
}

#[inline(always)]
/// Writes an OP_RUN to the memory address given.
/// Returns the number of bytes written.
/// # Safety
/// Assumes that allocation and length handling is handled outside of the function. This must be
/// handled outside of the function.
pub unsafe fn write_run(output_ptr: *mut u8, full_runs: usize, remainder: usize) -> usize {
    if full_runs > 0 {
        core::arch::asm!(
        "cld",
        "rep stosb",
        // these MUST be inout => _ because evil shenanigans happen when you don't clobber them
        inout("rcx") full_runs => _,
        inout("rdi") output_ptr => _,
        in("al") 0xfdu8,
        options(nostack)
        );
    }

    let remainder_exists = remainder > 0;
    if remainder_exists {
        let rem_op = QOI_OP_RUN | ((remainder as u8).wrapping_sub(1) & !QOI_OP_RUN);
        output_ptr.add(full_runs).write(rem_op);
    }
    debug_assert_ne!(full_runs + remainder, 0, "RUN called on no actual stuff");
    full_runs + remainder_exists as usize
}

pub trait Util<T: Sized> {
    unsafe fn get_write_head(&mut self) -> *mut T;
    unsafe fn add_len(&mut self, additional: usize);
    unsafe fn ptr_origin_distance(&self, other_ptr: *const T) -> isize;
    unsafe fn set_len_from_ptr(&mut self, end_ptr: *const T);
}

impl<T: Sized> Util<T> for Vec<T> {
    #[inline(always)]
    unsafe fn get_write_head(&mut self) -> *mut T {
        self.as_mut_ptr().add(self.len())
    }

    #[inline(always)]
    unsafe fn add_len(&mut self, additional: usize) {
        self.set_len(self.len() + additional);
    }

    #[inline(always)]
    unsafe fn ptr_origin_distance(&self, other_ptr: *const T) -> isize {
        other_ptr.offset_from(self.as_ptr())
    }

    #[inline(always)]
    unsafe fn set_len_from_ptr(&mut self, end_ptr: *const T) {
        self.set_len(self.ptr_origin_distance(end_ptr) as usize)
    }
}

pub trait NoPushByteWrite {
    unsafe fn push_var<T: Sized>(self, val: T) -> Self;
}

impl NoPushByteWrite for *mut u8 {
    #[inline(always)]
    unsafe fn push_var<T: Sized>(self, val: T) -> Self {
        (self as *mut T).write_unaligned(val);
        self.add(core::mem::size_of::<T>())
    }
}

#[macro_export]
macro_rules! prefetch {

    [$($address:ident $(+ $offset:literal)?),+] => {
        $(_mm_prefetch::<_MM_HINT_T0>($address$(.add($offset))? as *const i8);)*
    }
}

#[macro_export]
macro_rules! rotate {
    [$($vec:ident),+ @ const $num:literal] => {
        $($vec = _mm512_alignr_epi32::<$num>($vec, $vec);)+
    };

    [$($vec:ident),+ @ $amount:expr] => {
        $($vec = _mm512_maskz_compress_epi32(u16::MAX << $amount, $vec);)+
    };
}

#[inline]
pub unsafe fn no_rip_bcst_u32_m512(number: u32) -> Zmm {
    let rtn;
    core::arch::asm!(
    "vpbroadcastd {vec}, {num:e}",
    vec = out(zmm_reg) rtn,
    num = in(reg) number
    );
    rtn
}

#[inline]
pub unsafe fn actually_kmaskz_subb_si128(k: __mmask16, a: Xmm, b: Xmm) -> Xmm {
    let rtn;
    core::arch::asm!(
    "vpsubb {rtn} {{{mask}}} {{z}}, {a}, {b}",
    mask = in(kreg) k,
    a = in(xmm_reg) a,
    b = in(xmm_reg) b,
    rtn = out(xmm_reg) rtn,
    options(nostack, pure, nomem)
    );
    rtn
}

#[inline]
pub unsafe fn _mm512_cvtsi512_si64(v: Zmm) -> u64 {
    let rtn;
    core::arch::asm!(
    "vmovq {rtn}, {vec:x}",
    vec = in(zmm_reg) v,
    rtn = out(reg) rtn
    );
    rtn
}

#[inline]
pub unsafe fn xmm_of_low_dword(v: Zmm) -> Xmm {
    let new;
    core::arch::asm!(
    "vmovdqa32 {new:x} {{{one}}}, {v:x}",
    v = in(zmm_reg) v,
    one = in(kreg) 1,
    new = out(xmm_reg) new
    );
    new
}

pub enum RunResult {
    AtLeast(usize),
    Exactly(usize),
}

#[inline]
pub const fn run_length(mask: u16, rotation: u8) -> RunResult {
    use RunResult::{AtLeast, Exactly};
    let clobber_already_seen = mask << rotation;
    if clobber_already_seen == u16::MAX << rotation {
        AtLeast(16 - rotation as usize)
    } else {
        let clobbered = clobber_already_seen >> rotation;
        let how_many = clobbered.trailing_ones();
        Exactly(how_many as usize)
    }
}
