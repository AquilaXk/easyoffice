//! Per-presentation cache resolving `slide -> slideLayout -> slideMaster -> theme` `.rels`
//! chains, `<p:clrMap>`, and `<p:clrMapOvr>` with single-decode [`DecodeBudget`] accounting.

use std::collections::HashMap;

use crate::decode_budget::DecodeBudget;
use crate::ooxml_presentation::{resolve_ppt, utf8_entry};
use crate::pptx_theme_color::{PptxClrMap, PptxThemePalette};
use crate::rich_slide::SlideMasterLayout;
use crate::rtf_xml_helpers::{extract_attr_from_fragment, extract_balanced_elements};
use crate::xml_stream::{attr, XmlEvent, XmlReader};
use crate::{EasyOfficeError, ZipArchiveEntry};

const MAX_MASTER_LAYOUT_XML_EVENTS: usize = 131_072;

#[inline]
fn loc(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

struct CachedLayout {
    layout_type: SlideMasterLayout,
    master_path: Option<String>,
    clr_map_ovr: Option<PptxClrMap>,
}

struct CachedMaster {
    clr_map: PptxClrMap,
    theme_path: Option<String>,
}

/// Per-presentation cache of parsed `slideLayout`, `slideMaster`, and `theme` parts so
/// shared parts are decoded and charged to [`DecodeBudget`] at most once.
pub(crate) struct PptxThemeCache<'a> {
    map: &'a HashMap<&'a str, &'a ZipArchiveEntry>,
    pres_theme_path: Option<String>,
    layouts: HashMap<String, CachedLayout>,
    masters: HashMap<String, CachedMaster>,
    themes: HashMap<String, PptxThemePalette>,
}

impl<'a> PptxThemeCache<'a> {
    pub(crate) fn new(
        map: &'a HashMap<&'a str, &'a ZipArchiveEntry>,
        budget: &mut DecodeBudget,
    ) -> Result<Self, EasyOfficeError> {
        let pres_theme_path = find_rel_target_for_part(
            map,
            "ppt/_rels/presentation.xml.rels",
            "ppt",
            "/theme",
            budget,
        )?
        .or_else(|| {
            map.contains_key("ppt/theme/theme1.xml")
                .then(|| "ppt/theme/theme1.xml".to_string())
        });
        Ok(Self {
            map,
            pres_theme_path,
            layouts: HashMap::new(),
            masters: HashMap::new(),
            themes: HashMap::new(),
        })
    }

    pub(crate) fn resolve_for_slide(
        &mut self,
        layout_path: Option<&str>,
        slide_xml: &str,
        budget: &mut DecodeBudget,
    ) -> Result<(SlideMasterLayout, PptxThemePalette, PptxClrMap), EasyOfficeError> {
        let (layout_type, master_path, layout_ovr) = match layout_path {
            Some(lp) if self.map.contains_key(lp) => {
                let lay = self.load_layout(lp, budget)?;
                (lay.layout_type, lay.master_path.clone(), lay.clr_map_ovr)
            }
            _ => (SlideMasterLayout::TitleAndContent, None, None),
        };

        let (master_clr_map, theme_path) = match master_path.as_deref() {
            Some(mp) if self.map.contains_key(mp) => {
                let m = self.load_master(mp, budget)?;
                (m.clr_map, m.theme_path.clone())
            }
            _ => (PptxClrMap::default(), None),
        };

        let resolved_theme_path = theme_path.or_else(|| self.pres_theme_path.clone());
        let palette = match resolved_theme_path.as_deref() {
            Some(tp) if self.map.contains_key(tp) => self.load_theme(tp, budget)?,
            _ => PptxThemePalette::default(),
        };

        let mut clr_map = layout_ovr.unwrap_or(master_clr_map);
        if slide_xml.contains("overrideClrMapping") {
            apply_slide_clr_map_ovr(slide_xml, &mut clr_map)?;
        }
        Ok((layout_type, palette, clr_map))
    }

    fn load_layout(
        &mut self,
        path: &str,
        budget: &mut DecodeBudget,
    ) -> Result<&CachedLayout, EasyOfficeError> {
        if !self.layouts.contains_key(path) {
            let entry = self.map[path];
            let xml = utf8_entry(entry, path)?;
            budget.charge(xml.len(), "PPTX 레이아웃 XML")?;
            let (layout_type, clr_map_ovr) = parse_layout_meta(xml)?;
            let rels_path = part_rels_path(path);
            let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
            let master_path =
                find_rel_target_for_part(self.map, &rels_path, dir, "/slideMaster", budget)?;
            self.layouts.insert(
                path.to_string(),
                CachedLayout {
                    layout_type,
                    master_path,
                    clr_map_ovr,
                },
            );
        }
        Ok(&self.layouts[path])
    }

    fn load_master(
        &mut self,
        path: &str,
        budget: &mut DecodeBudget,
    ) -> Result<&CachedMaster, EasyOfficeError> {
        if !self.masters.contains_key(path) {
            let entry = self.map[path];
            let xml = utf8_entry(entry, path)?;
            budget.charge(xml.len(), "PPTX 마스터 XML")?;
            let clr_map = parse_master_clr_map(xml)?;
            let rels_path = part_rels_path(path);
            let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
            let theme_path = find_rel_target_for_part(self.map, &rels_path, dir, "/theme", budget)?;
            self.masters.insert(
                path.to_string(),
                CachedMaster {
                    clr_map,
                    theme_path,
                },
            );
        }
        Ok(&self.masters[path])
    }

    fn load_theme(
        &mut self,
        path: &str,
        budget: &mut DecodeBudget,
    ) -> Result<PptxThemePalette, EasyOfficeError> {
        if let Some(cached) = self.themes.get(path) {
            return Ok(cached.clone());
        }
        let entry = self.map[path];
        let xml = utf8_entry(entry, path)?;
        budget.charge(xml.len(), "PPTX 테마 XML")?;
        let palette = PptxThemePalette::from_theme_xml(xml)?;
        self.themes.insert(path.to_string(), palette.clone());
        Ok(palette)
    }
}

fn part_rels_path(part_path: &str) -> String {
    let (dir, file) = part_path.rsplit_once('/').unwrap_or(("", part_path));
    if dir.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{dir}/_rels/{file}.rels")
    }
}

fn find_rel_target_for_part(
    map: &HashMap<&str, &ZipArchiveEntry>,
    rels_path: &str,
    base_dir: &str,
    rel_suffix: &str,
    budget: &mut DecodeBudget,
) -> Result<Option<String>, EasyOfficeError> {
    let Some(&re) = map.get(rels_path) else {
        return Ok(None);
    };
    let r_xml = utf8_entry(re, rels_path)?;
    budget.charge(r_xml.len(), "PPTX 관계 XML")?;
    for (attrs, _) in extract_balanced_elements(r_xml, "Relationship") {
        if extract_attr_from_fragment(attrs, "TargetMode") == Some("External") {
            continue;
        }
        let rt = extract_attr_from_fragment(attrs, "Type").unwrap_or("");
        if !rt.ends_with(rel_suffix) {
            continue;
        }
        if let Some(target) = extract_attr_from_fragment(attrs, "Target") {
            let base = if target.starts_with("ppt/") {
                ""
            } else {
                base_dir
            };
            return Ok(Some(resolve_ppt(base, target)?));
        }
    }
    Ok(None)
}

fn parse_layout_meta(
    xml: &str,
) -> Result<(SlideMasterLayout, Option<PptxClrMap>), EasyOfficeError> {
    let mut reader = XmlReader::with_budget(xml, MAX_MASTER_LAYOUT_XML_EVENTS);
    let mut layout = SlideMasterLayout::TitleAndContent;
    let mut ovr = None;
    let mut in_clr_ovr = false;
    while let Some(ev) = reader.next_event()? {
        match ev {
            XmlEvent::Start { name, attrs } => match loc(name) {
                "sldLayout" => {
                    if let Some(lt) = attr(&attrs, "type") {
                        layout = SlideMasterLayout::from_ooxml_type(&lt);
                    }
                }
                "clrMapOvr" => in_clr_ovr = true,
                "overrideClrMapping" if in_clr_ovr => {
                    let mut m = PptxClrMap::default();
                    m.apply_attrs(&attrs);
                    ovr = Some(m);
                }
                _ => {}
            },
            XmlEvent::End(name) if loc(name) == "clrMapOvr" => in_clr_ovr = false,
            _ => {}
        }
    }
    Ok((layout, ovr))
}

fn parse_master_clr_map(xml: &str) -> Result<PptxClrMap, EasyOfficeError> {
    let mut reader = XmlReader::with_budget(xml, MAX_MASTER_LAYOUT_XML_EVENTS);
    let mut map = PptxClrMap::default();
    while let Some(ev) = reader.next_event()? {
        if let XmlEvent::Start { name, attrs } = ev {
            if loc(name) == "clrMap" {
                map.apply_attrs(&attrs);
                break;
            }
        }
    }
    Ok(map)
}

fn apply_slide_clr_map_ovr(
    slide_xml: &str,
    clr_map: &mut PptxClrMap,
) -> Result<(), EasyOfficeError> {
    let mut reader = XmlReader::with_budget(slide_xml, MAX_MASTER_LAYOUT_XML_EVENTS);
    let mut in_ovr = false;
    while let Some(ev) = reader.next_event()? {
        match ev {
            XmlEvent::Start { name, attrs } => match loc(name) {
                "clrMapOvr" => in_ovr = true,
                "overrideClrMapping" if in_ovr => {
                    clr_map.apply_attrs(&attrs);
                    break;
                }
                _ => {}
            },
            XmlEvent::End(name) if loc(name) == "clrMapOvr" => in_ovr = false,
            _ => {}
        }
    }
    Ok(())
}
