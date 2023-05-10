use core::arch::asm;

use super::VectorizedHashing;

// TODO: Find a way to actually make this faster than AVX
const RGBA_ARGB_SHUFFLE: [u8; 16] = [3, 0, 1, 2, 7, 4, 5, 6, 11, 8, 9, 10, 15, 12, 13, 14];
const HASH_MULTIPLIER_RGBA: u32 = 0x0b070503u32;
// const HASH_MULTIPLIER_ARGB: u32 = 0x07050b03u32;
const HASH_MULTIPLIER_ARGB: u32 = 0x0705030bu32;

const PUT_RG_LOW_BA_HIGH: [u16; 32] = [
    0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28,
    30, // gather the 3 * red + 5 * green in the lower half of the zmm
    1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29,
    31, // gather the 7 * blue + 11 * alpha in the upper half of the zmm
];
const HASH_MULTIPLIER_ARGB_WD_TOP6: u32 = HASH_MULTIPLIER_ARGB << 2;
const BIG_GATHER_WORDS: [u16; 32] = [
    00, 02, 04, 06, 08, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30, 32, 34, 36, 38, 40, 42, 44, 46,
    48, 50, 52, 54, 56, 58, 60, 62,
];

pub(crate) struct AVX512;

impl VectorizedHashing for AVX512 {
    unsafe fn hash_chunks(
        &self,
        pixel_ptr: *const u32,
        hash_ptr: *mut u8,
        count: usize,
    ) -> (*const u32, *mut u8) {
        hash_chunk_of_16_avx_512(pixel_ptr, hash_ptr, count)
    }

    fn hash_chunk_size(&self) -> usize {
        16
    }
}

#[inline]
pub unsafe fn hash_chunk_of_16_avx_512(
    mut pixel_read_ptr: *const u32,
    mut hash_write_ptr: *mut u8,
    count: usize,
) -> (*const u32, *mut u8) {
    asm!(
    // uses feature BW
    "vmovdqu16      {reorderer},    [{reorderer_ptr}]",
    "vpbroadcastd   {multipliers},  {multiplier:e}",
    "vpbroadcastw   {round_mask},   {byte_mask:e}",  // w because of the positioning

    "2:",
    "# LLVM-MCA-BEGIN 16-512",

    "vpmaddubsw     {partials},     {multipliers},  [{pixels_ptr}]",
    // This part is to compensate for the nonexistence of 512-bit horizontal add (which is stupid)
    "vpermw         {separate:z},   {reorderer},    {partials}",
    "vextracti64x4  {blue_alpha},   {separate:z},   1",

    "vpaddw     {hash_words},   {separate:y},   {blue_alpha}",
    "vpandd     {hash_words},   {hash_words},   {round_mask}",

    // actual output is equivalent to an xmm
    "vpmovwb    [{hashes_ptr}], {hash_words}",
    "lea        {pixels_ptr},   [{pixels_ptr} + 4*16]",
    "lea        {hashes_ptr},   [{hashes_ptr} + 1*16]",
    "cmp        {pixels_ptr},   {pixels_end_ptr}",

    "# LLVM-MCA-END 16-512",
    "jne 2b",

    multiplier  = in(reg)       HASH_MULTIPLIER_RGBA,
    byte_mask   = in(reg)       0x3f,

    pixels_ptr  = inout(reg)    pixel_read_ptr,
    hashes_ptr  = inout(reg)    hash_write_ptr,
    pixels_end_ptr = in(reg)    pixel_read_ptr.add(count * 16),
    reorderer_ptr  = in(reg)    &PUT_RG_LOW_BA_HIGH,

    multipliers = out(zmm_reg)  _,
    reorderer   = out(zmm_reg)  _,
    round_mask  = out(ymm_reg)  _,

    partials    = lateout(zmm_reg)  _,
    separate    = lateout(zmm_reg)  _,
    blue_alpha  = lateout(ymm_reg)  _,
    hash_words  = out(ymm_reg)      _,

    );
    return (pixel_read_ptr, hash_write_ptr);
}

#[inline]
pub unsafe fn hash_chunk_of_16_avx_512_wd(
    mut pixel_read_ptr: *const u32,
    mut hash_write_ptr: *mut u8,
    count: usize,
) -> (*const u32, *mut u8) {
    asm!(
    "vpbroadcastd   {rgba_argb},    [{rgba_argb_ptr}]",
    "vpbroadcastd   {multipliers},  {multiplier:e}",
    "vpbroadcastd   {round_mask},   {byte_mask:e}",  // w because of the positioning

    "2:",
    //"# LLVM-MCA-BEGIN 16-512-2",
    "vmovdqu32  {rgba},         [{pixels_ptr}]",
    "vpshufb    {argb},         {rgba},     {rgba_argb}",
    "vpmaddwd   {hash_dwords},  {argb},     {multipliers}",
    "vpsrld     {hash_dwords},  {hash_dwords},  8",
    "vpandd     {hash_dwords},  {hash_dwords},  {round_mask}",
    // actual output is equivalent to an xmm
    "vpmovdb    [{hashes_ptr}], {hash_dwords}",

    "lea        {pixels_ptr},   [{pixels_ptr} + 4*16]",
    "lea        {hashes_ptr},   [{hashes_ptr} + 1*16]",
    "cmp        {pixels_ptr},   {pixels_end_ptr}",

    //"# LLVM-MCA-END 16-512-2",
    "jne 2b",

    multiplier  = in(reg)       HASH_MULTIPLIER_ARGB,
    byte_mask   = in(reg)       0x3f,

    pixels_ptr  = inout(reg)    pixel_read_ptr,
    hashes_ptr  = inout(reg)    hash_write_ptr,
    pixels_end_ptr = in(reg)    pixel_read_ptr.add(count * 16),
    rgba_argb_ptr  = in(reg)    &RGBA_ARGB_SHUFFLE,


    multipliers = out(zmm_reg)  _,
    round_mask  = out(zmm_reg)  _,
    rgba_argb   = out(zmm_reg)  _,

    rgba        = lateout(zmm_reg) _,
    argb        = lateout(zmm_reg) _,
    hash_dwords = lateout(zmm_reg) _,

    );
    return (pixel_read_ptr, hash_write_ptr);
}

pub unsafe fn hash_chunk_of_32_avx_512_wd(
    mut pixel_read_ptr: *const u32,
    mut hash_write_ptr: *mut u8,
    count: usize,
) -> (*const u32, *mut u8) {
    asm!(
    "vpbroadcastd   {rgba_argb},    [{rgba_argb_ptr}]",
    "vpbroadcastd   {multipliers},  {multiplier:e}",
    "vpbroadcastd   {round_mask},   {byte_mask:e}",  // w because of the positioning
    "vmovdqu16      {gather},       [{gather_ptr}]",
    "# LLVM-MCA-BEGIN 32-512-2",
    "2:",

    "vmovdqu32  {rgba1},    [{pixels_ptr}]",
    "vmovdqu32  {rgba2},    [{pixels_ptr} + 4*16]",
    "vpshufb    {argb1},    {rgba1},    {rgba_argb}",
    "vpshufb    {argb2},    {rgba2},    {rgba_argb}",
    "vpmaddwd   {hashes},   {argb1},    {multipliers}",
    "vpmaddwd   {hashes2},  {argb2},    {multipliers}",
    "vpermt2w   {hashes},   {gather},   {hashes2}",

    "vpsrlw     {hashes},   {hashes},   10",

    // actual output is equivalent to a ymm
    "vpmovwb    [{hashes_ptr}], {hashes}",

    "lea        {pixels_ptr},   [{pixels_ptr} + 4*32]",
    "lea        {hashes_ptr},   [{hashes_ptr} + 1*32]",
    "cmp        {pixels_ptr},   {pixels_end_ptr}",

    "jne 2b",
    "# LLVM-MCA-END 32-512-2",
    multiplier  = in(reg)       HASH_MULTIPLIER_ARGB_WD_TOP6,
    byte_mask   = in(reg)       0x3f,

    pixels_ptr  = inout(reg)    pixel_read_ptr,
    hashes_ptr  = inout(reg)    hash_write_ptr,
    pixels_end_ptr = in(reg)    pixel_read_ptr.add(count * 16),
    rgba_argb_ptr  = in(reg)    &RGBA_ARGB_SHUFFLE,
    gather_ptr     = in(reg)    &BIG_GATHER_WORDS,


    multipliers = out(zmm_reg)  _,
    round_mask  = out(zmm_reg)  _,
    rgba_argb   = out(zmm_reg)  _,
    gather      = out(zmm_reg)  _,

    rgba1       = lateout(zmm_reg) _,
    rgba2       = lateout(zmm_reg) _,
    argb1       = lateout(zmm_reg) _,
    argb2       = lateout(zmm_reg) _,
    hashes      = lateout(zmm_reg) _,
    hashes2     = lateout(zmm_reg) _,

    );
    return (pixel_read_ptr, hash_write_ptr);
}
