use core::convert::From;
use core::ops::{Add, Sub};
use image::{Primitive, Rgba};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct WrappingRgba<T: Primitive>(Rgba<T>);
pub type PIXEL = WrappingRgba<u8>;

pub const ZERO_PIXEL: PIXEL = PIXEL {
    0: Rgba { 0: [0u8; 4] },
};

pub const BIAS_2: PIXEL = PIXEL {
    0: Rgba {
        0: [2u8, 2u8, 2u8, 0u8],
    },
};

pub const BLACK: PIXEL = PIXEL {
    0: Rgba {
        0: [0u8, 0u8, 0u8, 255u8],
    },
};

impl Add for PIXEL {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            0: Rgba {
                0: [
                    self.r().wrapping_add(other.r()),
                    self.g().wrapping_add(other.g()),
                    self.b().wrapping_add(other.b()),
                    self.a().wrapping_add(other.a()),
                ],
            },
        }
    }
}

impl Sub for PIXEL {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self {
            0: Rgba {
                0: [
                    self.r().wrapping_sub(other.r()),
                    self.g().wrapping_sub(other.g()),
                    self.b().wrapping_sub(other.b()),
                    self.a().wrapping_sub(other.a()),
                ],
            },
        }
    }
}

impl From<[u8; 4]> for PIXEL {
    fn from(bytes: [u8; 4]) -> Self {
        Self {
            0: Rgba { 0: bytes },
        }
    }
}

impl<T: Primitive> From<Rgba<T>> for WrappingRgba<T> {
    fn from(rgba: Rgba<T>) -> Self {
        Self { 0: rgba }
    }
}

impl<T: Primitive> WrappingRgba<T> {
    #[inline]
    pub fn r(&self) -> T {
        self.0 .0[0]
    }

    #[inline]
    pub fn g(&self) -> T {
        self.0 .0[1]
    }

    #[inline]
    pub fn b(&self) -> T {
        self.0 .0[2]
    }

    #[inline]
    pub fn a(&self) -> T {
        self.0 .0[3]
    }

    pub fn rgba_arr(&self) -> [T; 4] {
        self.0 .0
    }

    pub fn rgb_arr(&self) -> [T; 3] {
        self.0 .0[0..=2].try_into().unwrap()
    }
}
