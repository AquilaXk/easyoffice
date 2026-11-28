//! How a picture sits in the text, is cut and is turned (PD-395): the word processor's 그림 서식 > 텍스트 배치, 자르기 and 회전.
//! DOCX keeps them as `wp:inline` or `wp:anchor` with a wrap element, `a:srcRect` and the `rot` of `a:xfrm`. Every value read
//! from a file is checked here, so a picture in a document always holds a layout the writers can write.

/// Largest crop of one side, in thousandths of a percent of the picture (ECMA-376 20.1.10.40: 100000 is the whole side).
pub const MAX_CROP_SIDE: u32 = 99_000;

/// How the text goes around a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RichImageWrap {
    /// 줄 안: the picture is a character of its line.
    #[default]
    Inline,
    /// 정사각형: the text runs along the picture's box.
    Square,
    /// 위/아래: the text stops above and goes on below.
    TopBottom,
    /// 텍스트 뒤: the picture is behind the text.
    Behind,
    /// 텍스트 앞: the picture is over the text.
    InFront,
}

impl RichImageWrap {
    pub const ALL: [Self; 5] = [
        Self::Inline,
        Self::Square,
        Self::TopBottom,
        Self::Behind,
        Self::InFront,
    ];

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Square => "square",
            Self::TopBottom => "topBottom",
            Self::Behind => "behind",
            Self::InFront => "front",
        }
    }

    pub fn from_wire_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|wrap| wrap.wire_name() == name)
    }
}

/// The part of each side that is cut away, in thousandths of a percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RichImageCrop {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl RichImageCrop {
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }

    /// Each side within [`MAX_CROP_SIDE`] and at least 1% of the width and of the height left.
    pub fn is_valid(&self) -> bool {
        [self.left, self.top, self.right, self.bottom]
            .iter()
            .all(|side| *side <= MAX_CROP_SIDE)
            && self.left + self.right <= MAX_CROP_SIDE
            && self.top + self.bottom <= MAX_CROP_SIDE
    }
}

/// Wrap, crop and rotation of a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RichImageLayout {
    pub wrap: RichImageWrap,
    pub crop: RichImageCrop,
    /// Clockwise turn in degrees, `0..360`.
    pub rotation_degrees: u16,
}

impl RichImageLayout {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The layout when its crop and rotation are inside their bounds.
    pub fn checked(self) -> Option<Self> {
        (self.crop.is_valid() && self.rotation_degrees < 360).then_some(self)
    }
}

/// DrawingML angles are sixtieths of a degree (`rot="5400000"` is 90 degrees); the degrees, turned into `0..360`.
pub fn degrees_of_drawing_angle(angle: i64) -> u16 {
    (angle.rem_euclid(21_600_000) / 60_000) as u16
}

pub const fn drawing_angle_of_degrees(degrees: u16) -> u32 {
    degrees as u32 * 60_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_and_turns_stay_inside_their_bounds() {
        assert!(RichImageLayout::default().checked().is_some());
        let crop = |left, top, right, bottom| RichImageCrop {
            left,
            top,
            right,
            bottom,
        };
        assert!(crop(10_000, 0, 10_000, 0).is_valid());
        assert!(
            !crop(60_000, 0, 60_000, 0).is_valid(),
            "nothing left of the width"
        );
        assert!(!crop(100_000, 0, 0, 0).is_valid());
        assert!(RichImageLayout {
            rotation_degrees: 360,
            ..Default::default()
        }
        .checked()
        .is_none());
        assert_eq!(degrees_of_drawing_angle(5_400_000), 90);
        assert_eq!(degrees_of_drawing_angle(-5_400_000), 270);
        assert_eq!(degrees_of_drawing_angle(21_600_000 * 5 + 60_000), 1);
        assert_eq!(
            RichImageWrap::from_wire_name("front"),
            Some(RichImageWrap::InFront)
        );
        assert_eq!(RichImageWrap::from_wire_name("tight"), None);
    }
}
