use core::arch::asm;

use super::VectorizedHashing;

pub(crate) struct SSSE3;

impl VectorizedHashing for SSSE3 {
    unsafe fn hash_chunks(
        &self,
        pixel_ptr: *const u32,
        hash_ptr: *mut u8,
        count: usize,
    ) -> (*const u32, *mut u8) {
        hash_chunks_of_16(pixel_ptr, hash_ptr, count)
    }

    fn hash_chunk_size(&self) -> usize {
        16
    }
}

unsafe fn hash_chunks_of_16(
    mut pixel_read_ptr: *const u32,
    mut hash_write_ptr: *mut u8,
    chunk_count: usize,
) -> (*const u32, *mut u8) {
    asm!(
    "movddup    {multipliers},  {multipliers}",
    "movddup    {round_mask},   {round_mask}",

    "2:",
    // load 16 pixels into four xmm registers
    "movdqu     {pixels_a},     [{pixels_ptr}]",        // get b from chunk
    "movdqu     {pixels_b},     [{pixels_ptr} + 16]",   // get b from chunk
    "movdqu     {pixels_c},     [{pixels_ptr} + 32]",   // get c from chunk
    "movdqu     {pixels_d},     [{pixels_ptr} + 48]",   // get d from chunk

    // multiply and add all pairs of pixel channels simultaneously
    "pmaddubsw  {pixels_a},     {multipliers}",
    "pmaddubsw  {pixels_b},     {multipliers}",
    "pmaddubsw  {pixels_c},     {multipliers}",
    "pmaddubsw  {pixels_d},     {multipliers}",
    // horizontally add the channel pairs into final sums
    "phaddw     {pixels_a},     {pixels_b}",
    "phaddw     {pixels_c},     {pixels_d}",
    // cheating % 64
    "pand       {pixels_a},     {round_mask}", // a is now the hashes of the pixels originally in a and b
    "pand       {pixels_c},     {round_mask}", // c is now the hashes of the pixels originally in c and d

    "packuswb   {pixels_a},     {pixels_c}",   // a becomes the final 16 hashes in byte form
    "movdqu     [{hashes_ptr}], {pixels_a}",   // put a into list of hash results
    "lea        {pixels_ptr},   [{pixels_ptr} + 64]",
    "lea        {hashes_ptr},   [{hashes_ptr} + 16]",
    "cmp        {pixels_ptr},   {end_address}",
    "jne 2b",  // Loop until the stop address is reached (there are no more full pixel chunks)

    multipliers = in(xmm_reg)   HASH_MULTIPLIERS_RGBA,
    round_mask  = in(xmm_reg)   MOD_64_MASK,

    pixels_ptr  = inout(reg)    pixel_read_ptr,
    hashes_ptr  = inout(reg)    hash_write_ptr,
    end_address = in(reg)       pixel_read_ptr.add(chunk_count * 16),

    // probably best to let these be assigned by the assembler
    pixels_a    = out(xmm_reg)  _,
    pixels_b    = out(xmm_reg)  _,
    pixels_c    = out(xmm_reg)  _,
    pixels_d    = out(xmm_reg)  _,

    options(preserves_flags, nostack)
    );
    return (pixel_read_ptr, hash_write_ptr);
}

static MOD_64_MASK: u64 = 0x003f003f003f003fu64;
static HASH_MULTIPLIERS_RGBA: u64 = 0x0b0705030b070503u64;

unsafe fn hash_chunks_of_48(
    pixel_start_ptr: *const u32,
    hash_start_ptr: *mut u8,
    chunk_count: usize,
) -> (*const u32, *mut u8) {
    let pixel_chunk_end_ptr: *const u32;
    let hash_chunk_end_ptr: *mut u8;
    // This function is bigger than previous iterations of this idea, because it uses all most of
    // the available registers so that the CPU can do superscalar execution more readily.
    asm!(
    // First, the 64-bit constant values are duplicated into the high parts of their registers.
    "movddup    {multiplier}, {multiplier}",
    "movddup    {round_mask}, {round_mask}",

    "2:",  // marks the start of the loop
    "# LLVM-MCA-BEGIN 48",
    // 48 pixels are gathered in three batches.
    "movdqu     {px_b1_1},      [{pixels_ptr}]",
    "movdqu     {px_b1_2},      [{pixels_ptr} + 16]",
    "movdqu     {px_b1_3},      [{pixels_ptr} + 32]",
    "movdqu     {px_b1_4},      [{pixels_ptr} + 48]",
    // The batches do not exist, but they make it easier to think about.
    "movdqu     {px_b2_1},      [{pixels_ptr} + 64]",
    "movdqu     {px_b2_2},      [{pixels_ptr} + 80]",
    "movdqu     {px_b2_3},      [{pixels_ptr} + 96]",
    "movdqu     {px_b2_4},      [{pixels_ptr} + 112]",
    // Each batch will correspond to one output xmm of 16 hashes.
    "movdqu     {px_b3_1},      [{pixels_ptr} + 128]",
    "movdqu     {px_b3_2},      [{pixels_ptr} + 144]",
    "movdqu     {px_b3_3},      [{pixels_ptr} + 160]",
    "movdqu     {px_b3_4},      [{pixels_ptr} + 176]",

    // The multiplication and partial addition on each batch benefits the most from superscalar
    // execution, because each pmaddubsw takes 5 clocks, but this block of 12 pmaddubsw takes only
    // 10 clocks because multiple are able to be started in a pipe.
    "pmaddubsw  {px_b1_1},      {multiplier}",
    "pmaddubsw  {px_b1_2},      {multiplier}",
    "pmaddubsw  {px_b1_3},      {multiplier}",
    "pmaddubsw  {px_b1_4},      {multiplier}",

    "pmaddubsw  {px_b2_1},      {multiplier}",
    "pmaddubsw  {px_b2_2},      {multiplier}",
    "pmaddubsw  {px_b2_3},      {multiplier}",
    "pmaddubsw  {px_b2_4},      {multiplier}",

    "pmaddubsw  {px_b3_1},      {multiplier}",
    "pmaddubsw  {px_b3_2},      {multiplier}",
    "pmaddubsw  {px_b3_3},      {multiplier}",
    "pmaddubsw  {px_b3_4},      {multiplier}",

    // Both pairs of each batch are horizontally added to each other to complete the arithmetic.
    "phaddw     {px_b1_1},      {px_b1_2}",
    "phaddw     {px_b1_3},      {px_b1_4}",
    "phaddw     {px_b2_1},      {px_b2_2}",
    "phaddw     {px_b2_3},      {px_b2_4}",
    "phaddw     {px_b3_1},      {px_b3_2}",
    "phaddw     {px_b3_3},      {px_b3_4}",

    // Apply the mask that gets the modulo 64 of each hash
    "pand       {px_b1_1},      {round_mask}",
    "pand       {px_b1_3},      {round_mask}",
    "pand       {px_b2_1},      {round_mask}",
    "pand       {px_b2_3},      {round_mask}",
    "pand       {px_b3_1},      {round_mask}",
    "pand       {px_b3_3},      {round_mask}",

    // Pack each batch into its own xmm for writing to the hash vector.
    "packuswb   {px_b1_1},      {px_b1_3}",
    "packuswb   {px_b2_1},      {px_b2_3}",
    "packuswb   {px_b3_1},      {px_b3_3}",

    // Store all 48 hashes into the vector.
    "movdqa     [{hashes_ptr}],         {px_b1_1}",
    "movdqa     [{hashes_ptr} + 16],    {px_b2_1}",
    "movdqa     [{hashes_ptr} + 32],    {px_b3_1}",

    // Increment the pointers by one chunk
    "lea        {pixels_ptr},   [{pixels_ptr} + 48 * 4]",
    "lea        {hashes_ptr},   [{hashes_ptr} + 48 * 1]",
    "cmp        {pixels_ptr},   {end_address}",
    "# LLVM-MCA-END 48",
    "jne 2b",  // Loop until the stop address is reached (there are no more full pixel chunks)
    // Initial addresses
    pixels_ptr  = inout(reg) pixel_start_ptr => pixel_chunk_end_ptr,
    hashes_ptr  = inout(reg) hash_start_ptr => hash_chunk_end_ptr,
    end_address = in(reg)    pixel_start_ptr.add(chunk_count * 48),

    // Constants
    multiplier = in(xmm_reg) HASH_MULTIPLIERS_RGBA,
    round_mask = in(xmm_reg) MOD_64_MASK,

    // Batch 1
    px_b1_1 = out(xmm_reg) _,
    px_b1_2 = out(xmm_reg) _,
    px_b1_3 = out(xmm_reg) _,
    px_b1_4 = out(xmm_reg) _,
    // Batch 2
    px_b2_1 = out(xmm_reg) _,
    px_b2_2 = out(xmm_reg) _,
    px_b2_3 = out(xmm_reg) _,
    px_b2_4 = out(xmm_reg) _,
    // Batch 3
    px_b3_1 = out(xmm_reg) _,
    px_b3_2 = out(xmm_reg) _,
    px_b3_3 = out(xmm_reg) _,
    px_b3_4 = out(xmm_reg) _,

    options(preserves_flags, nostack)
    );
    return (pixel_chunk_end_ptr, hash_chunk_end_ptr);
}

static GATHER_HASHES: u64 = u64::from_ne_bytes([0, 2, 4, 6, 8, 10, 12, 14]);
static PACKED_ROUND_MASK: u64 = 0x3f3f3f3f3f3f3f3fu64;

pub unsafe fn hash_chunks_of_48_shuffle(
    pixel_start_ptr: *const u32,
    hash_start_ptr: *mut u8,
    chunk_count: usize,
) -> (*const u32, *mut u8) {
    let pixel_chunk_end_ptr: *const u32;
    let hash_chunk_end_ptr: *mut u8;
    // This function is bigger than previous iterations of this idea, because it uses most of
    // the available registers so that the CPU can do superscalar execution more readily.
    asm!(
    // First, the 64-bit constant values are duplicated into the high parts of their registers.
    "movddup    {multiplier}, {multiplier}",
    "movddup    {round_mask}, {round_mask}",

    "2:",  // marks the start of the loop
    "# LLVM-MCA-BEGIN 48s",
    // 48 pixels are gathered in three batches.
    "movdqu     {px_b1_1},      [{pixels_ptr}]",
    "movdqu     {px_b1_2},      [{pixels_ptr} + 16]",
    "movdqu     {px_b1_3},      [{pixels_ptr} + 32]",
    "movdqu     {px_b1_4},      [{pixels_ptr} + 48]",
    // The batches do not exist, but they make it easier to think about.
    "movdqu     {px_b2_1},      [{pixels_ptr} + 64]",
    "movdqu     {px_b2_2},      [{pixels_ptr} + 80]",
    "movdqu     {px_b2_3},      [{pixels_ptr} + 96]",
    "movdqu     {px_b2_4},      [{pixels_ptr} + 112]",
    // Each batch will correspond to one output xmm of 16 hashes.
    "movdqu     {px_b3_1},      [{pixels_ptr} + 128]",
    "movdqu     {px_b3_2},      [{pixels_ptr} + 144]",
    "movdqu     {px_b3_3},      [{pixels_ptr} + 160]",
    "movdqu     {px_b3_4},      [{pixels_ptr} + 176]",

    // Multiply and add pairs of bytes to have two halves of the hash in pair of words.
    "pmaddubsw  {px_b1_1},      {multiplier}",
    "pmaddubsw  {px_b1_2},      {multiplier}",
    "pmaddubsw  {px_b1_3},      {multiplier}",
    "pmaddubsw  {px_b1_4},      {multiplier}",

    "pmaddubsw  {px_b2_1},      {multiplier}",
    "pmaddubsw  {px_b2_2},      {multiplier}",
    "pmaddubsw  {px_b2_3},      {multiplier}",
    "pmaddubsw  {px_b2_4},      {multiplier}",

    "pmaddubsw  {px_b3_1},      {multiplier}",
    "pmaddubsw  {px_b3_2},      {multiplier}",
    "pmaddubsw  {px_b3_3},      {multiplier}",
    "pmaddubsw  {px_b3_4},      {multiplier}",

    // Each pair of words are horizontally added to each other to complete the arithmetic.
    "phaddw     {px_b1_1},      {px_b1_2}",
    "phaddw     {px_b1_3},      {px_b1_4}",
    "phaddw     {px_b2_1},      {px_b2_2}",
    "phaddw     {px_b2_3},      {px_b2_4}",
    "phaddw     {px_b3_1},      {px_b3_2}",
    "phaddw     {px_b3_3},      {px_b3_4}",

    // Gather all the hashes in the lower quadword of each register
    "pshufb     {px_b1_1},      {gather}",
    "pshufb     {px_b1_3},      {gather}",
    "pshufb     {px_b2_1},      {gather}",
    "pshufb     {px_b2_3},      {gather}",
    "pshufb     {px_b3_1},      {gather}",
    "pshufb     {px_b3_3},      {gather}",

    // Pack pairs of quadwords into registers.
    "punpcklqdq {px_b1_1},      {px_b1_3}",
    "punpcklqdq {px_b2_1},      {px_b2_3}",
    "punpcklqdq {px_b3_1},      {px_b3_3}",

    // Mask out the top two bits of each hash as a cheating % 64.
    "pand       {px_b1_1},      {round_mask}",
    "pand       {px_b2_1},      {round_mask}",
    "pand       {px_b3_1},      {round_mask}",

    // Store all 48 hashes into the Vec.
    "movdqa     [{hashes_ptr}],         {px_b1_1}",
    "movdqa     [{hashes_ptr} + 16],    {px_b2_1}",
    "movdqa     [{hashes_ptr} + 32],    {px_b3_1}",

    // Increment the pointers by one chunk
    "lea        {pixels_ptr},   [{pixels_ptr} + 48 * 4]",
    "lea        {hashes_ptr},   [{hashes_ptr} + 48 * 1]",
    "cmp        {pixels_ptr},   {end_address}",
    "# LLVM-MCA-END 48s",
    "jne 2b",  // Loop until the stop address is reached (there are no more full pixel chunks)
    // Initial addresses
    pixels_ptr  = inout(reg)    pixel_start_ptr => pixel_chunk_end_ptr,
    hashes_ptr  = inout(reg)    hash_start_ptr => hash_chunk_end_ptr,
    end_address = in(reg)       pixel_start_ptr.add(chunk_count * 48),

    // Constants
    multiplier  = in(xmm_reg)   HASH_MULTIPLIERS_RGBA,
    round_mask  = in(xmm_reg)   PACKED_ROUND_MASK,
    gather      = in(xmm_reg)   GATHER_HASHES,

    // Batch 1
    px_b1_1 = out(xmm_reg) _,
    px_b1_2 = out(xmm_reg) _,
    px_b1_3 = out(xmm_reg) _,
    px_b1_4 = out(xmm_reg) _,
    // Batch 2
    px_b2_1 = out(xmm_reg) _,
    px_b2_2 = out(xmm_reg) _,
    px_b2_3 = out(xmm_reg) _,
    px_b2_4 = out(xmm_reg) _,
    // Batch 3
    px_b3_1 = out(xmm_reg) _,
    px_b3_2 = out(xmm_reg) _,
    px_b3_3 = out(xmm_reg) _,
    px_b3_4 = out(xmm_reg) _,

    options(preserves_flags, nostack)
    );
    return (pixel_chunk_end_ptr, hash_chunk_end_ptr);
}

pub unsafe fn hash_chunk_of_16_intrin(
    pixel_ptr: *const u32,
    hash_ptr: *mut u8,
    count: usize,
) -> (*const u32, *mut u8) {
    #[cfg(target_arch = "x86")]
    use core::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    let mut pixel_ptr = pixel_ptr as *const __m128i;
    let mut hash_ptr = hash_ptr as *mut __m128i;
    let end_ptr = pixel_ptr.add(count);

    let multipliers = _mm_set1_epi32(i32::from_ne_bytes([3, 5, 7, 11]));
    let round_mask = _mm_set1_epi8(0x3f);

    while pixel_ptr != end_ptr {
        let pixels_a = _mm_loadu_si128(pixel_ptr);
        let pixels_b = _mm_loadu_si128(pixel_ptr.add(1));
        let pixels_c = _mm_loadu_si128(pixel_ptr.add(2));
        let pixels_d = _mm_loadu_si128(pixel_ptr.add(3));

        let partials_a = _mm_maddubs_epi16(pixels_a, multipliers);
        let partials_b = _mm_maddubs_epi16(pixels_b, multipliers);
        let partials_c = _mm_maddubs_epi16(pixels_c, multipliers);
        let partials_d = _mm_maddubs_epi16(pixels_d, multipliers);

        let unrounded_a = _mm_hadd_epi16(partials_a, partials_b);
        let unrounded_b = _mm_hadd_epi16(partials_c, partials_d);

        let rounded_a = _mm_and_si128(unrounded_a, round_mask);
        let rounded_b = _mm_and_si128(unrounded_b, round_mask);

        let packed_hashes = _mm_packus_epi16(rounded_a, rounded_b);

        _mm_stream_si128(hash_ptr, packed_hashes);

        pixel_ptr = pixel_ptr.add(4);
        hash_ptr = hash_ptr.add(1);
    }

    // Required before returning to code that may set atomic flags that invite concurrent reads,
    // as LLVM lowers `AtomicBool::store(flag, true, Release)` to ordinary stores on x86-64
    // instead of SFENCE, even though SFENCE is required in the presence of nontemporal stores.
    _mm_sfence();

    return (pixel_ptr as *const u32, hash_ptr as *mut u8);
}
