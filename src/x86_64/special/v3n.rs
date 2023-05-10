use core::arch::asm;

use super::{VectorizedHashing, HASH_MULTIPLIER_RGBA};

const MOVQB_REPLACEMENT: [u8; 16] = [
    0, 4, 8, 12, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128,
];
const CHUNK_REORDER: [u8; 8] = [0, 4, 2, 6, 1, 5, 3, 7];

pub(crate) struct AVXVNNI;

impl VectorizedHashing for AVXVNNI {
    unsafe fn hash_chunks(
        &self,
        pixel_ptr: *const u32,
        hash_ptr: *mut u8,
        count: usize,
    ) -> (*const u32, *mut u8) {
        hash_chunk_of_32_avx_vnni(pixel_ptr, hash_ptr, count)
    }

    fn hash_chunk_size(&self) -> usize {
        32
    }
}

// #[inline]
pub unsafe fn hash_chunk_of_32_avx_vnni(
    mut pixel_read_ptr: *const u32,
    mut hash_write_ptr: *mut u8,
    count: usize,
) -> (*const u32, *mut u8) {
    asm!(
    "vbroadcasti128 {gather},       [{movqb_ptr}]",
    "vpbroadcastd   {multipliers},  {multiplier:e}",
    "vpbroadcastb   {round_mask},   {byte_mask:e}",
    "vpmovzxbd      {reorder},      [{reorder_ptr}]",
    "2:",
    //"# LLVM-MCA-BEGIN 32nn",

    // load 32 pixels and take the dot product of the channels
    "vpdpbusd   {spaced_a},     {multipliers},  [{pixels_ptr}]",
    "vpdpbusd   {spaced_b},     {multipliers},  [{pixels_ptr} + 4*8]",
    "vpdpbusd   {spaced_c},     {multipliers},  [{pixels_ptr} + 4*16]",
    "vpdpbusd   {spaced_d},     {multipliers},  [{pixels_ptr} + 4*24]",

    "vpshufb    {lowds_a},  {spaced_a}, {gather}",
    "vpshufb    {lowds_b},  {spaced_b}, {gather}",
    "vpshufb    {lowds_c},  {spaced_c}, {gather}",
    "vpshufb    {lowds_d},  {spaced_d}, {gather}",

    "vpunpckldq {lowqs_a},  {lowds_a},  {lowds_b}",
    "vpunpckldq {lowqs_b},  {lowds_c},  {lowds_d}",
    "vpunpckldq {disorder}, {lowqs_a},  {lowqs_b}",
    "vpermd     {hashes},   {disorder}, {reorder}", // lane effect and unpacking has scrambled the order
    // cheating % 64
    "vpand      {hashes},   {hashes},   {round_mask}",

    "vmovntdq   [{hashes_ptr}], {hashes}",
    "lea        {pixels_ptr},   [{pixels_ptr} + 4*32]",
    "lea        {hashes_ptr},   [{hashes_ptr} + 1*32]",
    "cmp        {pixels_ptr},   {pixels_end_ptr}",
    //"# LLVM-MCA-END 32nn",
    "jne 2b",

    multiplier  = in(reg)   HASH_MULTIPLIER_RGBA,
    byte_mask   = in(reg)   0x3f,
    movqb_ptr   = in(reg)   &MOVQB_REPLACEMENT,
    reorder_ptr = in(reg)   &CHUNK_REORDER,

    pixels_ptr  = inout(reg)    pixel_read_ptr,
    hashes_ptr  = inout(reg)    hash_write_ptr,
    pixels_end_ptr = in(reg)    pixel_read_ptr.add(count * 32),

    // probably best to let these be assigned by the assembler
    spaced_a  = lateout(ymm_reg)  _,
    spaced_b  = lateout(ymm_reg)  _,
    spaced_c  = lateout(ymm_reg)  _,
    spaced_d  = lateout(ymm_reg)  _,

    lowds_a   = lateout(ymm_reg)  _,
    lowds_b   = lateout(ymm_reg)  _,
    lowds_c   = lateout(ymm_reg)  _,
    lowds_d   = lateout(ymm_reg)  _,

    lowqs_a   = lateout(ymm_reg)  _,
    lowqs_b   = lateout(ymm_reg)  _,

    disorder  = lateout(ymm_reg)  _,

    hashes  = out(ymm_reg)  _,

    round_mask  = out(ymm_reg)  _,
    multipliers = out(ymm_reg)  _,
    reorder     = out(ymm_reg)  _,
    gather      = out(ymm_reg)  _,

    options(preserves_flags, nostack)
    );
    return (pixel_read_ptr, hash_write_ptr);
}
