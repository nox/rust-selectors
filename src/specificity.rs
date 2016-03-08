/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::cmp;
use std::ops::Add;

const MAX_10BIT: u32 = (1u32 << 10) - 1;

/// A selector specificity.
#[cfg_attr(feature = "heap_size", derive(HeapSizeOf))]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Specificity(u32);

impl Add for Specificity {
    type Output = Specificity;

    #[inline]
    fn add(self, rhs: Specificity) -> Specificity {
        Specificity(
            cmp::min(self.0 & MAX_10BIT << 20 + rhs.0 & MAX_10BIT << 20, MAX_10BIT << 20)
            | cmp::min(self.0 & MAX_10BIT << 10 + rhs.0 & MAX_10BIT << 10, MAX_10BIT << 10)
            | cmp::min(self.0 & MAX_10BIT + rhs.0 & MAX_10BIT, MAX_10BIT))
    }
}

impl Default for Specificity {
    #[inline]
    fn default() -> Specificity {
        Specificity(0)
    }
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct UnpackedSpecificity {
    pub id_selectors: u32,
    pub class_like_selectors: u32,
    pub element_selectors: u32,
}

impl UnpackedSpecificity {
    #[inline]
    pub fn new(id_selectors: u32, class_like_selectors: u32, element_selectors: u32)
               -> UnpackedSpecificity {
        UnpackedSpecificity {
            id_selectors: id_selectors,
            class_like_selectors: class_like_selectors,
            element_selectors: element_selectors,
        }
    }
}

impl Add for UnpackedSpecificity {
    type Output = UnpackedSpecificity;

    #[inline]
    fn add(self, rhs: UnpackedSpecificity) -> UnpackedSpecificity {
        UnpackedSpecificity {
            id_selectors: self.id_selectors + rhs.id_selectors,
            class_like_selectors:
                self.class_like_selectors + rhs.class_like_selectors,
            element_selectors:
                self.element_selectors + rhs.element_selectors,
        }
    }
}

impl Default for UnpackedSpecificity {
    #[inline]
    fn default() -> UnpackedSpecificity {
        UnpackedSpecificity {
            id_selectors: 0,
            class_like_selectors: 0,
            element_selectors: 0,
        }
    }
}

impl From<UnpackedSpecificity> for Specificity {
    #[inline]
    fn from(specificity: UnpackedSpecificity) -> Specificity {
        Specificity(
            cmp::min(specificity.id_selectors, MAX_10BIT) << 20
            | cmp::min(specificity.class_like_selectors, MAX_10BIT) << 10
            | cmp::min(specificity.element_selectors, MAX_10BIT))
    }
}

impl From<Specificity> for UnpackedSpecificity {
    #[inline]
    fn from(specificity: Specificity) -> UnpackedSpecificity {
        UnpackedSpecificity {
            id_selectors: specificity.0 >> 20,
            class_like_selectors: specificity.0 >> 10 & MAX_10BIT,
            element_selectors: specificity.0 & MAX_10BIT,
        }
    }
}

/// Represents the bounds of a given value.
#[cfg_attr(feature = "heap_size", derive(HeapSizeOf))]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Bounds<T: Ord> {
    pub min: T,
    pub max: T,
}

impl Bounds<Specificity> {
    #[inline]
    pub fn unpack(self) -> Bounds<UnpackedSpecificity> {
        Bounds {
            min: self.min.into(),
            max: self.max.into(),
        }
    }
}

impl Bounds<UnpackedSpecificity> {
    #[inline]
    pub fn pack(self) -> Bounds<Specificity> {
        Bounds {
            min: self.min.into(),
            max: self.max.into(),
        }
    }

    #[inline]
    pub fn incr_element_selectors(&mut self) {
        self.min.element_selectors += 1;
        self.max.element_selectors += 1;
    }

    #[inline]
    pub fn incr_id_selectors(&mut self) {
        self.min.id_selectors += 1;
        self.max.id_selectors += 1;
    }

    #[inline]
    pub fn incr_class_like_selectors(&mut self) {
        self.min.class_like_selectors += 1;
        self.max.class_like_selectors += 1;
    }
}

impl<T: Ord> Bounds<T> {
    #[inline]
    pub fn widen(self, other: Self) -> Self {
        Bounds {
            min: cmp::min(self.min, other.min),
            max: cmp::max(self.max, other.max),
        }
    }
}

impl<T: Add<Output=T> + Copy + Ord> Add<T> for Bounds<T> {
    type Output = Bounds<T>;

    #[inline]
    fn add(self, other: T) -> Bounds<T> {
        Bounds {
            min: self.min + other,
            max: self.max + other,
        }
    }
}

impl<T: Add<Output=T> + Ord> Add for Bounds<T> {
    type Output = Bounds<T>;

    #[inline]
    fn add(self, other: Bounds<T>) -> Bounds<T> {
        Bounds {
            min: self.min + other.min,
            max: self.max + other.max,
        }
    }
}
