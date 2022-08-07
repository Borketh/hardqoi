use core::convert::From;
use core::ops::{Add, Sub};

#[derive(Clone, Copy)]
pub struct PIXEL(pub(crate) [u8; 4]);

pub const BIAS_2: PIXEL = PIXEL {
    0: [2u8, 2u8, 2u8, 0u8],
};
pub const BLACK: PIXEL = PIXEL {
    0: [0u8, 0u8, 0u8, 255u8],
};
pub const ZERO_PIXEL: PIXEL = PIXEL { 0: [0u8; 4] };

impl Add for PIXEL {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            0: [
                self.r().wrapping_add(other.r()),
                self.g().wrapping_add(other.g()),
                self.b().wrapping_add(other.b()),
                self.a().wrapping_add(other.a()),
            ],
        }
    }
}

impl Sub for PIXEL {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self {
            0: [
                self.r().wrapping_sub(other.r()),
                self.g().wrapping_sub(other.g()),
                self.b().wrapping_sub(other.b()),
                self.a().wrapping_sub(other.a()),
            ],
        }
    }
}

impl PartialEq<[u8; 4]> for PIXEL {
    fn eq(&self, other: &[u8; 4]) -> bool {
        self.0 == *other
    }
}

impl PartialEq<PIXEL> for PIXEL {
    fn eq(&self, other: &PIXEL) -> bool {
        self.0 == other.0
    }
}

impl From<[u8; 4]> for PIXEL {
    fn from(bytes: [u8; 4]) -> Self {
        Self { 0: bytes }
    }
}

#[cfg(feature = "image_compat")]
impl From<image::Rgba<u8>> for PIXEL {
    fn from(rgba: image::Rgba<u8>) -> Self {
        Self { 0: rgba.0 }
    }
}

impl PIXEL {
    #[inline]
    pub fn r(&self) -> u8 {
        self.0[0]
    }

    #[inline]
    pub fn g(&self) -> u8 {
        self.0[1]
    }

    #[inline]
    pub fn b(&self) -> u8 {
        self.0[2]
    }

    #[inline]
    pub fn a(&self) -> u8 {
        self.0[3]
    }

    pub fn rgba_arr(&self) -> [u8; 4] {
        self.0
    }

    pub fn rgb_arr(&self) -> [u8; 3] {
        self.0[0..=2].try_into().unwrap()
    }
}
