//! Slide model extensions kept beside the five element kinds: per-slide state (hidden flag, background,
//! header/footer fields, groups, animations) and per-element effects (rotation, flip, outline dash and
//! line, shadow, paragraph format). Effects hang off an element id so the element enum and its many
//! struct literals stay unchanged; every list is bounded and validated fail-closed.

use std::collections::{HashMap, HashSet};

use crate::rich_document::normalize_hex_color;
use crate::rich_slide::{
    SlideBox, SlideElement, MAX_ELEMENTS_PER_SLIDE, MAX_SLIDE_STROKE_PT, MAX_SLIDE_TEXT_BYTES,
};
use crate::{EasyOfficeError, RichImage};

pub const MAX_SLIDE_GROUPS: usize = 64;
pub const MAX_GROUP_NESTING: usize = 8;
pub const MAX_SLIDE_ANIMS: usize = 256;
pub const MAX_ANIM_MS: u32 = 60_000;
pub const DEFAULT_ANIM_MS: u32 = 500;
pub const MIN_LINE_SPACING_PCT: u16 = 50;
pub const MAX_LINE_SPACING_PCT: u16 = 500;
pub const MAX_SHADOW_PT: f64 = 100.0;
pub const MAX_PARA_SPACE_PT: f64 = 200.0;
pub const MAX_FOOTER_TEXT_BYTES: usize = 1_024;

macro_rules! str_enum {
    ($(#[$meta:meta])* $name:ident { $($var:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $($var),+ }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$var => $text),+ }
            }

            pub fn parse(s: &str) -> Option<Self> {
                match s.trim() { $($text => Some(Self::$var),)+ _ => None }
            }
        }
    };
}

str_enum! {
    /// Dash pattern of a shape or text box outline.
    SlideDash { Solid => "solid", Dash => "dash", Dot => "dot" }
}

str_enum! {
    /// Auto-number scheme of a numbered paragraph (`a:buAutoNum` type).
    SlideNumberStyle {
        ArabicPeriod => "arabicPeriod",
        ArabicParen => "arabicParenR",
        AlphaUpperPeriod => "alphaUcPeriod",
        AlphaLowerPeriod => "alphaLcPeriod",
        RomanUpperPeriod => "romanUcPeriod",
    }
}

str_enum! {
    /// What an animation does to its target: it appears, it is emphasized, or it leaves.
    AnimClass { Entrance => "entrance", Emphasis => "emphasis", Exit => "exit" }
}

str_enum! {
    AnimEffect {
        Appear => "appear",
        Fade => "fade",
        Fly => "fly",
        Wipe => "wipe",
        Zoom => "zoom",
        Spin => "spin",
        GrowShrink => "growShrink",
    }
}

str_enum! {
    /// When an animation starts relative to the one before it.
    AnimStart { OnClick => "onClick", WithPrevious => "withPrevious", AfterPrevious => "afterPrevious" }
}

str_enum! {
    /// Side the object comes from (fly, wipe) or goes to.
    AnimDir { Left => "left", Right => "right", Top => "top", Bottom => "bottom" }
}

// The enums come from `str_enum!`, which has no place to mark a `#[default]` variant.
#[allow(clippy::derivable_impls)]
impl Default for SlideDash {
    fn default() -> Self {
        Self::Solid
    }
}

#[allow(clippy::derivable_impls)]
impl Default for AnimDir {
    fn default() -> Self {
        Self::Bottom
    }
}

impl AnimClass {
    /// Effects the standard presentation menus offer for the class: appear, fade, fly, wipe, zoom for entrance and exit;
    /// spin and grow/shrink for emphasis.
    pub fn allows(self, effect: AnimEffect) -> bool {
        match self {
            Self::Emphasis => matches!(effect, AnimEffect::Spin | AnimEffect::GrowShrink),
            _ => !matches!(effect, AnimEffect::Spin | AnimEffect::GrowShrink),
        }
    }
}

/// Bullet of a paragraph (`a:buNone`, `a:buChar`, `a:buAutoNum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlideBullet {
    #[default]
    None,
    Char(char),
    Number(SlideNumberStyle),
}

/// Paragraph formatting applied to every paragraph of a text box or shape.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SlideParaFmt {
    pub justify: bool,
    pub bullet: SlideBullet,
    pub line_spacing_pct: Option<u16>,
    pub space_before_pt: Option<f64>,
    pub space_after_pt: Option<f64>,
}

impl SlideParaFmt {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    fn validate(&self) -> Result<(), EasyOfficeError> {
        if let SlideBullet::Char(c) = self.bullet {
            if c.is_control() || c.is_whitespace() {
                return Err(EasyOfficeError::InvalidArgument(
                    "슬라이드 글머리 기호는 보이는 한 글자여야 합니다.".to_string(),
                ));
            }
        }
        if let Some(p) = self.line_spacing_pct {
            if !(MIN_LINE_SPACING_PCT..=MAX_LINE_SPACING_PCT).contains(&p) {
                return Err(EasyOfficeError::InvalidArgument(format!(
                    "슬라이드 줄 간격은 {MIN_LINE_SPACING_PCT}~{MAX_LINE_SPACING_PCT}%여야 합니다."
                )));
            }
        }
        for v in [self.space_before_pt, self.space_after_pt]
            .into_iter()
            .flatten()
        {
            check_range(v, MAX_PARA_SPACE_PT, "슬라이드 문단 간격")?;
        }
        Ok(())
    }
}

/// Outline of a text box or picture (shapes keep their stroke on the element).
#[derive(Debug, Clone, PartialEq)]
pub struct SlideOutline {
    pub color: String,
    pub width_pt: f64,
}

/// Outer shadow (`a:outerShdw`): offset `dist_pt` along `angle_deg` (clockwise from the x axis).
#[derive(Debug, Clone, PartialEq)]
pub struct SlideShadow {
    pub color: String,
    pub alpha_pct: u8,
    pub blur_pt: f64,
    pub dist_pt: f64,
    pub angle_deg: f64,
}

/// Rotation, flip, outline, shadow and paragraph format of one element, found by its id.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ElementFx {
    pub id: String,
    pub rotation_deg: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub dash: SlideDash,
    pub outline: Option<SlideOutline>,
    pub shadow: Option<SlideShadow>,
    pub para: Option<SlideParaFmt>,
}

impl ElementFx {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            ..Self::default()
        }
    }

    /// True when nothing deviates from an element without effects, so it needs no entry.
    pub fn is_default(&self) -> bool {
        self.rotation_deg == 0.0
            && !self.flip_h
            && !self.flip_v
            && self.dash == SlideDash::Solid
            && self.outline.is_none()
            && self.shadow.is_none()
            && self.para.is_none_or(|p| p.is_default())
    }

    fn validate(&self) -> Result<(), EasyOfficeError> {
        if !self.rotation_deg.is_finite() || !(0.0..360.0).contains(&self.rotation_deg) {
            return Err(EasyOfficeError::InvalidArgument(
                "슬라이드 개체 회전각은 0 이상 360 미만이어야 합니다.".to_string(),
            ));
        }
        if let Some(o) = &self.outline {
            check_hex(&o.color, "outline")?;
            check_range(o.width_pt, MAX_SLIDE_STROKE_PT, "슬라이드 윤곽선 두께")?;
        }
        if let Some(s) = &self.shadow {
            check_hex(&s.color, "shadow")?;
            check_range(s.blur_pt, MAX_SHADOW_PT, "슬라이드 그림자 흐림")?;
            check_range(s.dist_pt, MAX_SHADOW_PT, "슬라이드 그림자 거리")?;
            if s.alpha_pct > 100 || !s.angle_deg.is_finite() || !(0.0..360.0).contains(&s.angle_deg)
            {
                return Err(EasyOfficeError::InvalidArgument(
                    "슬라이드 그림자 투명도는 0~100%, 각도는 0 이상 360 미만이어야 합니다."
                        .to_string(),
                ));
            }
        }
        self.para.map_or(Ok(()), |p| p.validate())
    }
}

/// Fill of a slide background.
#[derive(Debug, Clone, PartialEq)]
pub enum SlideBackground {
    Solid(String),
    /// Linear gradient from `from` to `to`; `angle_deg` runs clockwise from left to right.
    Gradient {
        from: String,
        to: String,
        angle_deg: u16,
    },
    Picture(RichImage),
}

/// Date field of the slide footer area.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SlideDate {
    #[default]
    None,
    Fixed(String),
    /// Updates to the current date when the slide is shown.
    Auto,
}

/// Slide number, date and footer text shown at the bottom of a slide.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlideFooter {
    pub number: bool,
    pub date: SlideDate,
    pub text: Option<String>,
}

impl SlideFooter {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Objects tied together; members are element ids or ids of other groups.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideGroup {
    pub id: String,
    pub members: Vec<String>,
}

/// One animation step on an element or group.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideAnim {
    pub target: String,
    pub class: AnimClass,
    pub effect: AnimEffect,
    pub start: AnimStart,
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub direction: AnimDir,
}

/// Per-slide extensions of [`crate::RichSlide`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlideExt {
    pub hidden: bool,
    pub background: Option<SlideBackground>,
    pub footer: SlideFooter,
    pub fx: Vec<ElementFx>,
    pub groups: Vec<SlideGroup>,
    pub anims: Vec<SlideAnim>,
}

impl SlideExt {
    pub fn fx_of(&self, id: &str) -> Option<&ElementFx> {
        self.fx.iter().find(|f| f.id == id)
    }

    /// Entry for `id`, created when the element had none.
    pub fn fx_mut(&mut self, id: &str) -> &mut ElementFx {
        if let Some(i) = self.fx.iter().position(|f| f.id == id) {
            return &mut self.fx[i];
        }
        self.fx.push(ElementFx::new(id));
        self.fx.last_mut().expect("just pushed")
    }

    pub fn group_of(&self, id: &str) -> Option<&SlideGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// Ids of groups no other group contains, in the order they were created.
    pub fn top_groups(&self) -> Vec<&SlideGroup> {
        let inner: HashSet<&str> = self
            .groups
            .iter()
            .flat_map(|g| g.members.iter().map(String::as_str))
            .collect();
        self.groups
            .iter()
            .filter(|g| !inner.contains(g.id.as_str()))
            .collect()
    }

    pub fn background_image_bytes(&self) -> usize {
        match &self.background {
            Some(SlideBackground::Picture(img)) => img.bytes().len(),
            _ => 0,
        }
    }

    /// Drops entries that default away and entries for elements that no longer exist.
    pub fn prune(&mut self, element_ids: &HashSet<&str>) {
        self.fx
            .retain(|f| element_ids.contains(f.id.as_str()) && !f.is_default());
        self.groups.retain(|g| !g.members.is_empty());
        let known: HashSet<String> = element_ids
            .iter()
            .map(|s| s.to_string())
            .chain(self.groups.iter().map(|g| g.id.clone()))
            .collect();
        self.anims.retain(|a| known.contains(&a.target));
    }

    fn known_ids<'a>(&'a self, element_ids: &HashSet<&'a str>) -> HashSet<&'a str> {
        let mut known = element_ids.clone();
        known.extend(self.groups.iter().map(|g| g.id.as_str()));
        known
    }

    /// Enforces the slide budgets and that fx, group and animation entries refer to real objects.
    pub fn validate(&self, element_ids: &HashSet<&str>) -> Result<(), EasyOfficeError> {
        if self.fx.len() > MAX_ELEMENTS_PER_SLIDE
            || self.groups.len() > MAX_SLIDE_GROUPS
            || self.anims.len() > MAX_SLIDE_ANIMS
        {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "슬라이드 효과·그룹·애니메이션 수가 상한({MAX_ELEMENTS_PER_SLIDE}/{MAX_SLIDE_GROUPS}/{MAX_SLIDE_ANIMS})을 초과했습니다."
            )));
        }
        let mut seen = HashSet::new();
        for fx in &self.fx {
            if !element_ids.contains(fx.id.as_str()) || !seen.insert(fx.id.as_str()) {
                return Err(EasyOfficeError::InvalidArgument(format!(
                    "슬라이드 개체 효과의 대상 '{}'이 없거나 중복입니다.",
                    fx.id
                )));
            }
            fx.validate()?;
        }
        self.validate_groups(element_ids)?;
        let known = self.known_ids(element_ids);
        for a in &self.anims {
            if !known.contains(a.target.as_str()) {
                return Err(EasyOfficeError::InvalidArgument(format!(
                    "애니메이션 대상 '{}'이 슬라이드에 없습니다.",
                    a.target
                )));
            }
            if !a.class.allows(a.effect) || a.duration_ms > MAX_ANIM_MS || a.delay_ms > MAX_ANIM_MS
            {
                return Err(EasyOfficeError::InvalidArgument(
                    "애니메이션 효과 종류나 시간이 올바르지 않습니다.".to_string(),
                ));
            }
        }
        self.validate_look()
    }

    fn validate_groups(&self, element_ids: &HashSet<&str>) -> Result<(), EasyOfficeError> {
        let group_ids: HashSet<&str> = self.groups.iter().map(|g| g.id.as_str()).collect();
        if group_ids.len() != self.groups.len() || !group_ids.is_disjoint(element_ids) {
            return Err(EasyOfficeError::InvalidArgument(
                "슬라이드 그룹 ID가 중복이거나 개체 ID와 겹칩니다.".to_string(),
            ));
        }
        let mut parent: HashMap<&str, &str> = HashMap::new();
        for g in &self.groups {
            if g.members.len() < 2 || g.members.len() > MAX_ELEMENTS_PER_SLIDE {
                return Err(EasyOfficeError::InvalidArgument(format!(
                    "그룹 '{}'은 개체 2개 이상으로 이뤄져야 합니다.",
                    g.id
                )));
            }
            for m in &g.members {
                let known = element_ids.contains(m.as_str()) || group_ids.contains(m.as_str());
                if !known || parent.insert(m.as_str(), g.id.as_str()).is_some() {
                    return Err(EasyOfficeError::InvalidArgument(format!(
                        "그룹 '{}'의 구성원 '{m}'이 없거나 다른 그룹에도 속합니다.",
                        g.id
                    )));
                }
            }
        }
        for g in &self.groups {
            let (mut cur, mut depth) = (g.id.as_str(), 0usize);
            while let Some(&p) = parent.get(cur) {
                depth += 1;
                if depth > MAX_GROUP_NESTING {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "그룹 중첩이 상한({MAX_GROUP_NESTING})을 넘거나 순환합니다."
                    )));
                }
                cur = p;
            }
        }
        Ok(())
    }

    fn validate_look(&self) -> Result<(), EasyOfficeError> {
        match &self.background {
            Some(SlideBackground::Solid(c)) => check_hex(c, "background")?,
            Some(SlideBackground::Gradient {
                from,
                to,
                angle_deg,
            }) => {
                check_hex(from, "background")?;
                check_hex(to, "background")?;
                if *angle_deg >= 360 {
                    return Err(EasyOfficeError::InvalidArgument(
                        "슬라이드 배경 그라데이션 각도는 0 이상 360 미만이어야 합니다.".to_string(),
                    ));
                }
            }
            _ => {}
        }
        let text_len = self.footer.text.as_deref().map_or(0, str::len);
        let date_len = match &self.footer.date {
            SlideDate::Fixed(t) => t.len(),
            _ => 0,
        };
        if text_len.max(date_len) > MAX_FOOTER_TEXT_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "슬라이드 바닥글 길이가 상한({MAX_FOOTER_TEXT_BYTES}B)을 초과했습니다."
            )));
        }
        Ok(())
    }
}

fn check_hex(raw: &str, field: &str) -> Result<(), EasyOfficeError> {
    if normalize_hex_color(raw).is_none() {
        return Err(EasyOfficeError::InvalidArgument(format!(
            "슬라이드 '{field}'는 RRGGBB/#RRGGBB 색이어야 합니다: {raw}"
        )));
    }
    Ok(())
}

fn check_range(v: f64, max: f64, label: &str) -> Result<(), EasyOfficeError> {
    if !v.is_finite() || !(0.0..=max).contains(&v) {
        return Err(EasyOfficeError::InvalidArgument(format!(
            "{label}는 0~{max}pt여야 합니다."
        )));
    }
    Ok(())
}

/// One entry of a slide's drawing order: an element (by position) or a group with its members in order.
#[derive(Debug, Clone, PartialEq)]
pub enum SlideNode<'a> {
    Leaf(usize),
    Group {
        id: &'a str,
        children: Vec<SlideNode<'a>>,
    },
}

impl SlideNode<'_> {
    fn min_leaf(&self) -> usize {
        match self {
            Self::Leaf(i) => *i,
            Self::Group { children, .. } => children.iter().map(Self::min_leaf).min().unwrap_or(0),
        }
    }
}

impl SlideExt {
    /// Elements in z-order with each top-level group written where its first member sits.
    pub fn layout<'a>(&'a self, elements: &[SlideElement]) -> Vec<SlideNode<'a>> {
        let index: HashMap<&str, usize> = elements
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id(), i))
            .collect();
        let mut owner: Vec<Option<&SlideGroup>> = vec![None; elements.len()];
        for g in self.top_groups() {
            for i in self.leaves_of(&g.id, &index) {
                owner[i] = Some(g);
            }
        }
        let mut done: HashSet<&str> = HashSet::new();
        let mut out = Vec::with_capacity(elements.len());
        for (i, group) in owner.into_iter().enumerate() {
            match group {
                None => out.push(SlideNode::Leaf(i)),
                Some(g) if done.insert(g.id.as_str()) => out.push(self.group_node(g, &index)),
                Some(_) => {}
            }
        }
        out
    }

    fn leaves_of(&self, id: &str, index: &HashMap<&str, usize>) -> Vec<usize> {
        if let Some(&i) = index.get(id) {
            return vec![i];
        }
        self.group_of(id).map_or_else(Vec::new, |g| {
            g.members
                .iter()
                .flat_map(|m| self.leaves_of(m, index))
                .collect()
        })
    }

    fn group_node<'a>(&'a self, g: &'a SlideGroup, index: &HashMap<&str, usize>) -> SlideNode<'a> {
        let mut kids: Vec<SlideNode<'a>> = g
            .members
            .iter()
            .filter_map(|m| match index.get(m.as_str()) {
                Some(&i) => Some(SlideNode::Leaf(i)),
                None => self.group_of(m).map(|sub| self.group_node(sub, index)),
            })
            .collect();
        kids.sort_by_key(SlideNode::min_leaf);
        SlideNode::Group {
            id: &g.id,
            children: kids,
        }
    }

    /// Smallest upright box around a node, counting the rotation of every element in it.
    pub fn node_bounds(&self, node: &SlideNode<'_>, elements: &[SlideElement]) -> SlideBox {
        match node {
            SlideNode::Leaf(i) => {
                let el = &elements[*i];
                let rad = self
                    .fx_of(el.id())
                    .map_or(0.0, |f| f.rotation_deg)
                    .to_radians();
                let (sin, cos) = (rad.sin().abs(), rad.cos().abs());
                let r = el.rect();
                let (w, h) = (r.w * cos + r.h * sin, r.w * sin + r.h * cos);
                SlideBox {
                    x: r.x + (r.w - w) / 2.0,
                    y: r.y + (r.h - h) / 2.0,
                    w,
                    h,
                }
            }
            SlideNode::Group { children, .. } => children
                .iter()
                .map(|c| self.node_bounds(c, elements))
                .reduce(|a, b| {
                    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
                    SlideBox {
                        x,
                        y,
                        w: (a.x + a.w).max(b.x + b.w) - x,
                        h: (a.y + a.h).max(b.y + b.h) - y,
                    }
                })
                .unwrap_or(SlideBox {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                }),
        }
    }
}

impl SlideElement {
    pub fn set_id(&mut self, new_id: String) {
        match self {
            Self::TextBox { id, .. }
            | Self::Shape { id, .. }
            | Self::Image { id, .. }
            | Self::Table { id, .. }
            | Self::Chart { id, .. } => *id = new_id,
        }
    }

    pub fn set_rect(&mut self, new_rect: SlideBox) {
        match self {
            Self::TextBox { rect, .. }
            | Self::Shape { rect, .. }
            | Self::Image { rect, .. }
            | Self::Table { rect, .. }
            | Self::Chart { rect, .. } => *rect = new_rect,
        }
    }
}

/// Longest text a footer field accepts, shared by the JSON and file readers.
pub fn clamp_footer_text(raw: &str) -> String {
    let mut end = raw
        .len()
        .min(MAX_FOOTER_TEXT_BYTES.min(MAX_SLIDE_TEXT_BYTES));
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    raw[..end].to_string()
}
