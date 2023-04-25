use super::VectorizedHashing;
use core::arch::asm;
const HASH_MULTIPLIER_RGBA: u32 = 0x0b070503u32;

pub(crate) struct AVX512VNNI;

impl VectorizedHashing for AVX512VNNI {
    unsafe fn hash_chunks(
        &self,
        pixel_ptr: *const u32,
        hash_ptr: *mut u8,
        count: usize,
    ) -> (*const u32, *mut u8) {
        hash_chunk_of_160_avx_512_vnni(pixel_ptr, hash_ptr, count)
    }

    fn hash_chunk_size(&self) -> usize {
        160
    }
}

// TODO: This borders on absurdity to achieve a p/c greater than the other implementations. Much work needed.
#[inline]
pub unsafe fn hash_chunk_of_160_avx_512_vnni(
    mut pixel_read_ptr: *const u32,
    mut hash_write_ptr: *mut u8,
    count: usize,
) -> (*const u32, *mut u8) {
    todo!();
    asm!(
    // uses feature BW
    "vpbroadcastd   {multipliers},  {multiplier:e}",
    "vpbroadcastd   {round_mask},   {byte_mask:e}",  // d because of the positioning

    "2:",
    //"# LLVM-MCA-BEGIN 160-512-VNNI",

    "vpdpbusd   {hashes_0}, {multipliers},  [{pixels_ptr}]",
    "vpdpbusd   {hashes_1}, {multipliers},  [{pixels_ptr} + 4*16]",
    "vpdpbusd   {hashes_2}, {multipliers},  [{pixels_ptr} + 4*32]",
    "vpdpbusd   {hashes_3}, {multipliers},  [{pixels_ptr} + 4*48]",
    "vpdpbusd   {hashes_4}, {multipliers},  [{pixels_ptr} + 4*64]",
    "vpdpbusd   {hashes_5}, {multipliers},  [{pixels_ptr} + 4*80]",
    "vpdpbusd   {hashes_6}, {multipliers},  [{pixels_ptr} + 4*96]",
    "vpdpbusd   {hashes_7}, {multipliers},  [{pixels_ptr} + 4*112]",
    "vpdpbusd   {hashes_8}, {multipliers},  [{pixels_ptr} + 4*128]",
    "vpdpbusd   {hashes_9}, {multipliers},  [{pixels_ptr} + 4*144]",

    "vpandd     {hashes_0}, {hashes_0},     {round_mask}",
    "vpandd     {hashes_1}, {hashes_1},     {round_mask}",
    "vpandd     {hashes_2}, {hashes_2},     {round_mask}",
    "vpandd     {hashes_3}, {hashes_3},     {round_mask}",
    "vpandd     {hashes_4}, {hashes_4},     {round_mask}",
    "vpandd     {hashes_5}, {hashes_5},     {round_mask}",
    "vpandd     {hashes_6}, {hashes_6},     {round_mask}",
    "vpandd     {hashes_7}, {hashes_7},     {round_mask}",
    "vpandd     {hashes_8}, {hashes_8},     {round_mask}",
    "vpandd     {hashes_9}, {hashes_9},     {round_mask}",

    // actual outputs are equivalent xmms
    // also, this horrible bastard has rth of 2
    "vpmovdb    [{hashes_ptr}],         {hashes_0}",
    "vpmovdb    [{hashes_ptr} + 1*16],  {hashes_1}",
    "vpmovdb    [{hashes_ptr} + 1*32],  {hashes_2}",
    "vpmovdb    [{hashes_ptr} + 1*48],  {hashes_3}",
    "vpmovdb    [{hashes_ptr} + 1*64],  {hashes_4}",
    "vpmovdb    [{hashes_ptr} + 1*80],  {hashes_5}",
    "vpmovdb    [{hashes_ptr} + 1*96],  {hashes_6}",
    "vpmovdb    [{hashes_ptr} + 1*112], {hashes_7}",
    "vpmovdb    [{hashes_ptr} + 1*128], {hashes_8}",
    "vpmovdb    [{hashes_ptr} + 1*144], {hashes_9}",

    "lea        {pixels_ptr},   [{pixels_ptr} + 4*160]",
    "lea        {hashes_ptr},   [{hashes_ptr} + 1*160]",
    "cmp        {pixels_ptr},   {pixels_end_ptr}",

    //"# LLVM-MCA-END 160-512-VNNI",
    "jne 2b",

    multiplier  = in(reg)       HASH_MULTIPLIER_RGBA,
    byte_mask   = in(reg)       0x3f,

    pixels_ptr  = inout(reg)    pixel_read_ptr,
    hashes_ptr  = inout(reg)    hash_write_ptr,
    pixels_end_ptr = in(reg)    pixel_read_ptr.add(count * 16),

    multipliers = out(zmm_reg)  _,
    round_mask  = out(zmm_reg)  _,

    hashes_0 = out(zmm_reg) _,
    hashes_1 = out(zmm_reg) _,
    hashes_2 = out(zmm_reg) _,
    hashes_3 = out(zmm_reg) _,
    hashes_4 = out(zmm_reg) _,
    hashes_5 = out(zmm_reg) _,
    hashes_6 = out(zmm_reg) _,
    hashes_7 = out(zmm_reg) _,
    hashes_8 = out(zmm_reg) _,
    hashes_9 = out(zmm_reg) _,
    );
    return (pixel_read_ptr, hash_write_ptr);
}
