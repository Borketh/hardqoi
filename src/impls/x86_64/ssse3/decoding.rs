use crate::alloc::vec::Vec;
use crate::common::{
    QOIHeader, END_8, QOI_OP_DIFF, QOI_OP_INDEX, QOI_OP_LUMA, QOI_OP_RGB, QOI_OP_RGBA, QOI_OP_RUN,
};
use core::hint::unreachable_unchecked;
#[path = "decode_context.rs"]
mod decode_context;
use decode_context::DecodeContext;

#[inline(never)]
pub fn decode(input: &Vec<u8>, output: &mut Vec<[u8; 4]>) -> Result<(), (usize, usize)> {
    let header = QOIHeader::from(<[u8; 14]>::from(input[0..14].try_into().unwrap()));
    output.reserve_exact(header.image_size());
    let mut ctx: DecodeContext = DecodeContext::new(input, output);

    let len: usize = input.len() - 8;

    // if the first op is a run, black ends up not in the HIA because of the hash-skipping behaviour
    if DecodeContext::is_run(ctx.get_byte()) {
        // this fixes that
        ctx.hia_push_prev();
    }

    while ctx.pos() < len {
        let next_op: u8 = ctx.get_byte();

        match next_op {
            QOI_OP_RGBA => unsafe {
                ctx.load_some_rgba();
            },
            QOI_OP_RGB => unsafe {
                ctx.load_one_rgb();
            },
            // it turns out that the compiler can make this into a LUT without me manually doing so
            _ => match next_op & 0b11000000 {
                QOI_OP_DIFF => unsafe {
                    ctx.load_diff();
                },
                QOI_OP_LUMA => unsafe {
                    ctx.load_one_luma();
                },
                QOI_OP_RUN => unsafe {
                    ctx.load_run();
                },
                QOI_OP_INDEX => unsafe {
                    ctx.load_index();
                },
                _ => unsafe { unreachable_unchecked() },
            },
        } // end match 8-bit
    } // end loop

    let pos = ctx.pos();

    debug_assert_eq!(
        input[(pos)..(pos + 8)],
        END_8,
        "QOI file does not end normally! Found {:?} instead",
        &input[(pos)..(pos + 8)]
    );

    if header.image_size() == output.len() {
        Ok(())
    } else {
        Err((output.len(), header.image_size()))
    }
}
