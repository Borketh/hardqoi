use core::arch::asm;

use super::VectorizedHashing;

const HASH_MULTIPLIER_RGBA: u32 = 0x0b070503;

const MASK_64: u32 = 0x003f; // should be u16 but broadcast only takes r or e regs
const REORDERING_INDICES: [u8; 8] = [0, 4, 1, 5, 2, 6, 3, 7];

pub(crate) struct AVX;

impl VectorizedHashing for AVX {
    unsafe fn hash_chunks(
        &self,
        pixel_ptr: *const u32,
        hash_ptr: *mut u8,
        count: usize,
    ) -> (*const u32, *mut u8) {
        hash_chunk_of_32_avx(pixel_ptr, hash_ptr, count)
    }

    fn hash_chunk_size(&self) -> usize {
        32
    }
}

#[inline]
pub unsafe fn hash_chunk_of_32_avx(
    mut pixel_read_ptr: *const u32,
    mut hash_write_ptr: *mut u8,
    count: usize,
) -> (*const u32, *mut u8) {
    asm!(
    "vpbroadcastd   {multipliers},  [{multiplier}]",
    "vpbroadcastw   {round_mask},   [{byte_mask}]",
    "vpmovzxbd      {reorder},      [{reorder_ptr}]",
    "2:",
    "# LLVM-MCA-BEGIN 32avx",

    // load 32 pixels and multiply and add all pairs of pixel channels simultaneously
    "vpmaddubsw {partials_a},   {multipliers},  [{pixels_ptr}]",
    "vpmaddubsw {partials_b},   {multipliers},  [{pixels_ptr} + 4*8]",
    "vpmaddubsw {partials_c},   {multipliers},  [{pixels_ptr} + 4*16]",
    "vpmaddubsw {partials_d},   {multipliers},  [{pixels_ptr} + 4*24]",
    // horizontally add the channel pairs into final sums
    "vphaddw    {hashes_a},     {partials_a},   {partials_b}",
    "vphaddw    {hashes_b},     {partials_c},   {partials_d}",
    // cheating % 64
    "vpand      {hashes_a},     {hashes_a},     {round_mask}",
    "vpand      {hashes_b},     {hashes_b},     {round_mask}",
    // mask has to happen before the pack because packuswb saturates numbers >255 to 255
    "vpackuswb  {all_hashes},   {hashes_a},     {hashes_b}",
    "vpermd     {all_hashes},   {reorder},      {all_hashes}",
    "vmovdqu    [{hashes_ptr}], {all_hashes}",
    "lea        {pixels_ptr},   [{pixels_ptr} + 4*32]",
    "lea        {hashes_ptr},   [{hashes_ptr} + 1*32]",
    "# LLVM-MCA-END 32avx",
    "cmp        {pixels_ptr},   {pixels_end_ptr}",
    "jne 2b",

    multiplier  = in(reg)       &HASH_MULTIPLIER_RGBA,
    byte_mask   = in(reg)       &MASK_64,
    reorder_ptr = in(reg)       &REORDERING_INDICES,

    pixels_ptr  = inout(reg)    pixel_read_ptr,
    hashes_ptr  = inout(reg)    hash_write_ptr,
    pixels_end_ptr = in(reg)    pixel_read_ptr.add(count * 32),

    // probably best to let these be assigned by the assembler
    partials_a  = lateout(ymm_reg)  _,
    partials_b  = lateout(ymm_reg)  _,
    partials_c  = lateout(ymm_reg)  _,
    partials_d  = lateout(ymm_reg)  _,

    hashes_a    = lateout(ymm_reg)  _,
    hashes_b    = lateout(ymm_reg)  _,

    all_hashes  = out(ymm_reg)  _,

    round_mask  = out(ymm_reg)  _,
    multipliers = out(ymm_reg)  _,
    reorder     = out(ymm_reg)  _,

    options(preserves_flags, nostack)
    );
    return (pixel_read_ptr, hash_write_ptr);
}
