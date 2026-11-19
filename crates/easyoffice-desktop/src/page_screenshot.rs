//! Headless multi-page PNG screenshot CLI (`PD-258`): renders `HWP`/`HWPX`, `PDF`,
//! `PPTX`/`PPSX`/`ODP`/`PPT`, `DOCX`/`ODT`/`RTF`/`MD`/`TXT`/`Pages`/`DOC`, and
//! `XLSX`/`XLS`/`ODS`/`CSV`/`TSV` pages into bounded PNG files without opening any GUI window.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use easyoffice_formats::{
    encode_sheet_pdf, encode_slide_pdf, CellValueKind, DecodedContent, DocumentFormat,
    FormatDetector, OdsUnpacked, PdfWriteOptions, RichSlideDeck, SheetStyling,
    UniversalFormatConverter, XmlSanitizer,
};
use easyoffice_sheet::{
    EvaluatedCellValue, PackedCellCoordinate, WorkbookEngine, WorkbookEvalTarget,
};
use hayro::hayro_interpret::font::{FontData, FontQuery};
use hayro::hayro_interpret::hayro_cmap::CidFamily;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::kurbo::Affine;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{render_into, RenderCache, RenderSettings};
use rhwp::DocumentCore;
use serde::Serialize;
use usvg::fontdb::{Database, Family, Query};

use crate::pdf_edit_budget::{Deadline, MAX_PAGES};
use crate::pdf_edit_inspect::{open_within, Limits};
use crate::pdf_edit_wire::MAX_RENDER_FONT_BYTES;
use crate::pdf_export;
use crate::pdf_font_fallbacks::add_font_fallbacks;
use crate::pdf_fonts::shared_font_db;
use crate::raster_budget::RasterBudget;
use crate::sensitive_path::{ensure_safe_symlink_target, in_sensitive_credential_dir};

pub const MAX_SCREENSHOT_FILE_BYTES: usize = 64 * 1_048_576;
pub const MAX_SCREENSHOT_SVG_BYTES: usize = 64 * 1_048_576;
pub const MAX_SCREENSHOT_PAGES: usize = 200;
pub const MAX_SCREENSHOT_SIDE_PX: u32 = 4_096;
pub const DEFAULT_SCREENSHOT_DPI: u32 = 144;
pub const MIN_SCREENSHOT_DPI: u32 = 36;
pub const MAX_SCREENSHOT_DPI: u32 = 300;
pub const SCREENSHOT_DEADLINE: Duration = Duration::from_secs(30);

const WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;
const MIN_PAGE_SIDE_PT: f32 = 1.0;

#[derive(Debug)]
enum ScreenshotError {
    Usage(String),
    Runtime(String),
}

impl ScreenshotError {
    #[inline]
    fn usage(msg: impl Into<String>) -> Self {
        Self::Usage(msg.into())
    }
    #[inline]
    fn rt(msg: impl Into<String>) -> Self {
        Self::Runtime(msg.into())
    }
    fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::Runtime(_) => 1,
        }
    }
    fn message(&self) -> &str {
        match self {
            Self::Usage(m) | Self::Runtime(m) => m,
        }
    }
}

struct ScreenshotArgs {
    input_path: PathBuf,
    out_dir: PathBuf,
    pages: Option<Vec<usize>>,
    dpi: u32,
}

#[derive(Clone, Copy)]
struct RenderCtx<'a> {
    pages: Option<&'a [usize]>,
    dpi: u32,
    fonts: &'a Arc<Database>,
    cjk_font: Option<&'a [u8]>,
    out_dir: &'a Path,
    deadline: &'a Deadline,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderedPageEntry {
    page: usize,
    file_name: String,
    width_px: u32,
    height_px: u32,
    bytes: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScreenshotSummary<'a> {
    status: &'static str,
    format: &'a str,
    page_count: usize,
    dpi: u32,
    rendered_count: usize,
    files: Vec<RenderedPageEntry>,
}

/// Entry hook invoked before Tauri initializes; returns `Some(exit_code)` when screenshot CLI
/// flags are present so no GUI window is ever opened.
pub fn maybe_run_cli() -> Option<i32> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if !has_screenshot_flag(&args) {
        return None;
    }
    match run_cli(&args) {
        Ok(json) => {
            println!("{json}");
            Some(0)
        }
        Err(err) => {
            eprintln!("error: {}", err.message());
            Some(err.exit_code())
        }
    }
}

fn has_screenshot_flag(args: &[OsString]) -> bool {
    args.iter().any(|raw| {
        let Some(s) = raw.to_str() else {
            return false;
        };
        let key = s.split_once('=').map_or(s, |(k, _)| k);
        matches!(
            key,
            "--render-pages" | "--screenshot" | "--out-dir" | "--pages" | "--dpi"
        )
    })
}

fn parse_dpi(raw: &str) -> Result<u32, ScreenshotError> {
    let dpi = raw.trim().parse::<u32>().map_err(|_| {
        ScreenshotError::usage(format!(
            "--dpi 값이 올바르지 않습니다({MIN_SCREENSHOT_DPI}~{MAX_SCREENSHOT_DPI})."
        ))
    })?;
    if !(MIN_SCREENSHOT_DPI..=MAX_SCREENSHOT_DPI).contains(&dpi) {
        return Err(ScreenshotError::usage(format!(
            "--dpi 값({dpi})은 {MIN_SCREENSHOT_DPI}..={MAX_SCREENSHOT_DPI} 범위여야 합니다."
        )));
    }
    Ok(dpi)
}

fn parse_page_spec(raw: &str) -> Result<Vec<usize>, ScreenshotError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ScreenshotError::usage("--pages 값이 비어 있습니다."));
    }
    let mut selected = BTreeSet::new();
    for segment in trimmed.split(',') {
        let part = segment.trim();
        if part.is_empty() {
            return Err(ScreenshotError::usage("--pages 목록에 빈 항목이 있습니다."));
        }
        if let Some((a_raw, b_raw)) = part.split_once('-') {
            let start = a_raw
                .trim()
                .parse::<usize>()
                .map_err(|_| ScreenshotError::usage("--pages 범위 시작이 올바르지 않습니다."))?;
            let end = b_raw
                .trim()
                .parse::<usize>()
                .map_err(|_| ScreenshotError::usage("--pages 범위 끝이 올바르지 않습니다."))?;
            if start == 0 || end == 0 || start > end {
                return Err(ScreenshotError::usage(
                    "--pages 범위는 1 이상의 오름차순(예: 2-4)이어야 합니다.",
                ));
            }
            let span = end - start + 1;
            if span > MAX_SCREENSHOT_PAGES
                || selected.len().saturating_add(span) > MAX_SCREENSHOT_PAGES
            {
                return Err(ScreenshotError::usage(format!(
                    "--pages 선택 쪽 수가 상한({MAX_SCREENSHOT_PAGES})을 초과합니다."
                )));
            }
            selected.extend(start..=end);
        } else {
            let page = part
                .parse::<usize>()
                .map_err(|_| ScreenshotError::usage("--pages 쪽 번호가 올바르지 않습니다."))?;
            if page == 0 {
                return Err(ScreenshotError::usage(
                    "--pages 쪽 번호는 1 이상이어야 합니다.",
                ));
            }
            if !selected.contains(&page) && selected.len() >= MAX_SCREENSHOT_PAGES {
                return Err(ScreenshotError::usage(format!(
                    "--pages 선택 쪽 수가 상한({MAX_SCREENSHOT_PAGES})을 초과합니다."
                )));
            }
            selected.insert(page);
        }
    }
    Ok(selected.into_iter().collect())
}

fn take_opt_val(
    args: &[OsString],
    idx: &mut usize,
    opt: &str,
    inline: Option<&str>,
) -> Result<OsString, ScreenshotError> {
    if let Some(val) = inline {
        return if val.is_empty() {
            Err(ScreenshotError::usage(format!(
                "옵션 '{opt}'에는 값이 필요합니다."
            )))
        } else {
            Ok(OsString::from(val))
        };
    }
    let next = args
        .get(*idx + 1)
        .filter(|v| {
            !v.is_empty()
                && !v
                    .to_str()
                    .is_some_and(|s| s.starts_with('-') && s.len() > 1)
        })
        .ok_or_else(|| ScreenshotError::usage(format!("옵션 '{opt}'에는 값이 필요합니다.")))?;
    *idx += 1;
    Ok(next.clone())
}

fn parse_cli_args(args: &[OsString]) -> Result<ScreenshotArgs, ScreenshotError> {
    let (mut input_path, mut out_dir, mut pages, mut dpi) = (None, None, None, None);
    let mut i = 0usize;
    while i < args.len() {
        let raw = args[i]
            .to_str()
            .ok_or_else(|| ScreenshotError::usage("유효하지 않은 UTF-8 명령줄 인자입니다."))?;
        let (key, inline) = match raw.split_once('=') {
            Some((k, v)) if k.starts_with("--") => (k, Some(v)),
            _ => (raw, None),
        };
        match key {
            "--render-pages" | "--screenshot" => {
                if input_path.is_some() {
                    return Err(ScreenshotError::usage(
                        "--render-pages 옵션이 중복되었습니다.",
                    ));
                }
                input_path = Some(PathBuf::from(take_opt_val(args, &mut i, key, inline)?));
            }
            "--out-dir" => {
                if out_dir.is_some() {
                    return Err(ScreenshotError::usage("--out-dir 옵션이 중복되었습니다."));
                }
                out_dir = Some(PathBuf::from(take_opt_val(args, &mut i, key, inline)?));
            }
            "--pages" => {
                if pages.is_some() {
                    return Err(ScreenshotError::usage("--pages 옵션이 중복되었습니다."));
                }
                let val = take_opt_val(args, &mut i, key, inline)?;
                pages =
                    Some(parse_page_spec(val.to_str().ok_or_else(|| {
                        ScreenshotError::usage("--pages UTF-8 오류")
                    })?)?);
            }
            "--dpi" => {
                if dpi.is_some() {
                    return Err(ScreenshotError::usage("--dpi 옵션이 중복되었습니다."));
                }
                let val = take_opt_val(args, &mut i, key, inline)?;
                dpi =
                    Some(parse_dpi(val.to_str().ok_or_else(|| {
                        ScreenshotError::usage("--dpi UTF-8 오류")
                    })?)?);
            }
            _ => {
                return Err(ScreenshotError::usage(format!(
                    "알 수 없는 인자입니다: {raw}"
                )))
            }
        }
        i += 1;
    }
    Ok(ScreenshotArgs {
        input_path: input_path
            .ok_or_else(|| ScreenshotError::usage("--render-pages <FILE> 인자가 필요합니다."))?,
        out_dir: out_dir
            .ok_or_else(|| ScreenshotError::usage("--out-dir <DIR> 인자가 필요합니다."))?,
        pages,
        dpi: dpi.unwrap_or(DEFAULT_SCREENSHOT_DPI),
    })
}

fn resolve_requested_pages(
    spec: Option<&[usize]>,
    page_count: usize,
) -> Result<Vec<usize>, ScreenshotError> {
    if page_count == 0 {
        return Err(ScreenshotError::rt("문서에 렌더링할 쪽이 없습니다."));
    }
    match spec {
        Some(req) => {
            if let Some(&bad) = req.iter().find(|&&p| p > page_count) {
                return Err(ScreenshotError::usage(format!(
                    "요청한 쪽({bad})이 전체 쪽 수({page_count})를 초과합니다."
                )));
            }
            Ok(req.to_vec())
        }
        None if page_count > MAX_SCREENSHOT_PAGES => Err(ScreenshotError::usage(format!(
            "전체 쪽 수({page_count})가 1회 상한({MAX_SCREENSHOT_PAGES})을 초과합니다."
        ))),
        None => Ok((1..=page_count).collect()),
    }
}

fn screenshot_font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut push_if_dir = |p: PathBuf| {
        if p.is_dir() && !dirs.contains(&p) {
            dirs.push(p);
        }
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            push_if_dir(exe_dir.join("pdf-fonts"));
            if let Some(parent) = exe_dir.parent() {
                for sub in [
                    "Resources/pdf-fonts",
                    "pdf-fonts",
                    "share/easyoffice/pdf-fonts",
                    "lib/EasyOffice/pdf-fonts",
                ] {
                    push_if_dir(parent.join(sub));
                }
            }
            let mut cur = Some(exe_dir);
            for _ in 0..5 {
                if let Some(dir) = cur {
                    push_if_dir(dir.join("third_party/rhwp/ttfs/opensource"));
                    cur = dir.parent();
                }
            }
        }
    }
    push_if_dir(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/rhwp/ttfs/opensource"),
    );
    #[cfg(target_os = "linux")]
    push_if_dir(PathBuf::from("/mnt/c/Windows/Fonts"));
    dirs
}

fn extract_cjk_font_bytes(fonts: &Database) -> Option<Vec<u8>> {
    let q_noto = Query {
        families: &[Family::Name("Noto Sans KR")],
        ..Query::default()
    };
    let q_sans = Query {
        families: &[Family::SansSerif],
        ..Query::default()
    };
    let id = fonts.query(&q_noto).or_else(|| fonts.query(&q_sans))?;
    fonts
        .with_face_data(id, |data, idx| {
            (idx == 0 && data.len() <= MAX_RENDER_FONT_BYTES).then(|| data.to_vec())
        })
        .flatten()
}

fn read_bounded_input(path: &Path) -> Result<Vec<u8>, ScreenshotError> {
    let _ = fs::symlink_metadata(path)
        .map_err(|_| ScreenshotError::rt("입력 파일을 확인할 수 없습니다."))?;
    let canonical = fs::canonicalize(path)
        .map_err(|_| ScreenshotError::rt("입력 파일 경로를 해석할 수 없습니다."))?;
    ensure_safe_symlink_target(&canonical).map_err(|e| ScreenshotError::rt(e.to_string()))?;
    let meta = fs::metadata(&canonical)
        .map_err(|_| ScreenshotError::rt("입력 파일 정보를 읽을 수 없습니다."))?;
    if !meta.is_file() {
        return Err(ScreenshotError::rt("일반 파일만 렌더링할 수 있습니다."));
    }
    if meta.len() > MAX_SCREENSHOT_FILE_BYTES as u64 {
        return Err(ScreenshotError::rt(
            "입력 파일 크기가 64 MiB 한도를 초과했습니다.",
        ));
    }
    let file =
        File::open(&canonical).map_err(|_| ScreenshotError::rt("입력 파일을 열 수 없습니다."))?;
    let mut buf = Vec::with_capacity((meta.len() as usize).min(MAX_SCREENSHOT_FILE_BYTES));
    file.take((MAX_SCREENSHOT_FILE_BYTES + 1) as u64)
        .read_to_end(&mut buf)
        .map_err(|_| ScreenshotError::rt("입력 파일을 읽는 중 오류가 발생했습니다."))?;
    if buf.is_empty() {
        return Err(ScreenshotError::rt("입력 파일이 비어 있습니다."));
    }
    if buf.len() > MAX_SCREENSHOT_FILE_BYTES {
        return Err(ScreenshotError::rt(
            "입력 파일 크기가 64 MiB 한도를 초과했습니다.",
        ));
    }
    Ok(buf)
}

fn prepare_output_dir(out_dir: &Path) -> Result<PathBuf, ScreenshotError> {
    if in_sensitive_credential_dir(out_dir) {
        return Err(ScreenshotError::rt(
            "자격증명 디렉터리에는 출력할 수 없습니다.",
        ));
    }
    let sym_opt = fs::symlink_metadata(out_dir).ok();
    if sym_opt.is_none() {
        fs::create_dir_all(out_dir)
            .map_err(|_| ScreenshotError::rt("출력 디렉터리를 생성할 수 없습니다."))?;
    }
    let canon = fs::canonicalize(out_dir)
        .map_err(|_| ScreenshotError::rt("출력 디렉터리 경로를 해석할 수 없습니다."))?;
    if sym_opt.is_some_and(|s| s.file_type().is_symlink()) {
        ensure_safe_symlink_target(&canon).map_err(|e| ScreenshotError::rt(e.to_string()))?;
    }
    if in_sensitive_credential_dir(&canon) {
        return Err(ScreenshotError::rt(
            "자격증명 디렉터리에는 출력할 수 없습니다.",
        ));
    }
    let meta = fs::metadata(&canon)
        .map_err(|_| ScreenshotError::rt("출력 디렉터리 정보를 확인할 수 없습니다."))?;
    if !meta.is_dir() {
        return Err(ScreenshotError::rt("출력 경로가 디렉터리가 아닙니다."));
    }
    Ok(canon)
}

fn write_rendered_page(
    canon_out: &Path,
    page: usize,
    w_px: u32,
    h_px: u32,
    png_bytes: &[u8],
) -> Result<RenderedPageEntry, ScreenshotError> {
    let file_name = format!("page-{page:04}.png");
    let target = canon_out.join(&file_name);
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(ScreenshotError::rt(
                "출력 대상이 심볼릭 링크 또는 비일반 파일이어서 쓰기를 차단했습니다.",
            ));
        }
    }
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = opts
        .open(&target)
        .map_err(|_| ScreenshotError::rt("PNG 출력 파일을 열 수 없습니다."))?;
    file.write_all(png_bytes)
        .map_err(|_| ScreenshotError::rt("PNG 출력 파일에 쓸 수 없습니다."))?;
    Ok(RenderedPageEntry {
        page,
        file_name,
        width_px: w_px,
        height_px: h_px,
        bytes: png_bytes.len(),
    })
}

fn clamp_pixel_dimensions(raw_w: f32, raw_h: f32) -> Result<(u32, u32), ScreenshotError> {
    if !(raw_w.is_finite() && raw_h.is_finite() && raw_w > 0.0 && raw_h > 0.0) {
        return Err(ScreenshotError::rt("쪽 크기가 올바르지 않습니다."));
    }
    let max_side = MAX_SCREENSHOT_SIDE_PX as f32;
    let clamp = if raw_w > max_side || raw_h > max_side {
        (max_side / raw_w).min(max_side / raw_h)
    } else {
        1.0
    };
    Ok((
        (raw_w * clamp).round().clamp(1.0, max_side) as u32,
        (raw_h * clamp).round().clamp(1.0, max_side) as u32,
    ))
}

fn render_svg_at_dpi(
    svg: &str,
    fonts: &Arc<Database>,
    dpi: u32,
) -> Result<(Vec<u8>, u32, u32), ScreenshotError> {
    if svg.len() > MAX_SCREENSHOT_SVG_BYTES {
        return Err(ScreenshotError::rt(
            "쪽 SVG가 허용 크기 한도를 초과했습니다.",
        ));
    }
    let svg = add_font_fallbacks(svg);
    XmlSanitizer::validate_safe_xml(&svg)
        .map_err(|e| ScreenshotError::rt(format!("쪽 SVG를 읽지 못했습니다: {e}")))?;
    crate::svg_budget::check_svg_structure(&svg)
        .map_err(|e| ScreenshotError::rt(format!("쪽 SVG를 읽지 못했습니다: {e}")))?;
    let tree = usvg::Tree::from_str(
        &svg,
        &pdf_export::bitmap_parse_options(fonts, RasterBudget::for_page()),
    )
    .map_err(|e| ScreenshotError::rt(format!("쪽 SVG를 파싱하지 못했습니다: {e}")))?;
    let (svg_w, svg_h) = (tree.size().width(), tree.size().height());
    let raw_scale = dpi as f32 / 96.0;
    let (w_px, h_px) = clamp_pixel_dimensions(svg_w * raw_scale, svg_h * raw_scale)?;
    let mut pixmap = tiny_skia::Pixmap::new(w_px, h_px)
        .ok_or_else(|| ScreenshotError::rt("PNG 픽스맵을 생성하지 못했습니다."))?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(w_px as f32 / svg_w, h_px as f32 / svg_h),
        &mut pixmap.as_mut(),
    );
    let png = pixmap
        .encode_png()
        .map_err(|e| ScreenshotError::rt(format!("PNG 인코딩 실패: {e}")))?;
    Ok((png, w_px, h_px))
}

enum RhwpStreamMsg {
    PageCount(usize),
    PageSvg { page: usize, svg: String },
    Error(ScreenshotError),
}

fn render_rhwp_pages(
    bytes: &[u8],
    ctx: RenderCtx<'_>,
) -> Result<(usize, Vec<RenderedPageEntry>), ScreenshotError> {
    let spec_owned = ctx.pages.map(<[usize]>::to_vec);
    let deadline_copy = *ctx.deadline;
    thread::scope(|scope| {
        let (tx, rx) = mpsc::sync_channel::<RhwpStreamMsg>(2);
        let handle = thread::Builder::new()
            .name("eo-screenshot-rhwp".into())
            .stack_size(WORKER_STACK_BYTES)
            .spawn_scoped(scope, move || {
                let core = match DocumentCore::from_bytes(bytes) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(RhwpStreamMsg::Error(ScreenshotError::rt(format!(
                            "한글 문서를 배치하지 못했습니다: {e}"
                        ))));
                        return;
                    }
                };
                let page_count = core.page_count() as usize;
                let selected = match resolve_requested_pages(spec_owned.as_deref(), page_count) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = tx.send(RhwpStreamMsg::Error(e));
                        return;
                    }
                };
                if tx.send(RhwpStreamMsg::PageCount(page_count)).is_err() {
                    return;
                }
                for page in selected {
                    if let Err(msg) = deadline_copy.check() {
                        let _ = tx.send(RhwpStreamMsg::Error(ScreenshotError::rt(msg)));
                        return;
                    }
                    match core.render_page_svg_native((page - 1) as u32) {
                        Ok(svg) => {
                            if tx.send(RhwpStreamMsg::PageSvg { page, svg }).is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(RhwpStreamMsg::Error(ScreenshotError::rt(format!(
                                "{page}쪽 SVG 생성 실패: {e}"
                            ))));
                            return;
                        }
                    }
                }
            })
            .map_err(|e| ScreenshotError::rt(format!("렌더링 스레드 시작 실패: {e}")))?;

        let (mut total_pages, mut rendered) = (None, Vec::new());
        for msg in rx {
            match msg {
                RhwpStreamMsg::PageCount(n) => total_pages = Some(n),
                RhwpStreamMsg::PageSvg { page, svg } => {
                    ctx.deadline.check().map_err(ScreenshotError::Runtime)?;
                    let (png, w_px, h_px) = render_svg_at_dpi(&svg, ctx.fonts, ctx.dpi)?;
                    rendered.push(write_rendered_page(ctx.out_dir, page, w_px, h_px, &png)?);
                }
                RhwpStreamMsg::Error(err) => {
                    let _ = handle.join();
                    return Err(err);
                }
            }
        }
        handle
            .join()
            .map_err(|_| ScreenshotError::rt("문서 렌더링 중 오류가 발생했습니다."))?;
        let page_count =
            total_pages.ok_or_else(|| ScreenshotError::rt("문서 쪽 수를 확인하지 못했습니다."))?;
        Ok((page_count, rendered))
    })
}

fn pdf_interpreter_settings(cjk_font: Option<&[u8]>) -> InterpreterSettings {
    let cjk_font: Option<FontData> = cjk_font.map(|b| Arc::new(b.to_vec()) as FontData);
    let mut settings = InterpreterSettings::default();
    let stock = Arc::clone(&settings.font_resolver);
    settings.font_resolver = Arc::new(move |query| {
        if let (FontQuery::Fallback(fallback), Some(font)) = (query, &cjk_font) {
            let cjk = fallback.character_collection.as_ref().is_some_and(|c| {
                matches!(
                    c.family,
                    CidFamily::AdobeKorea1
                        | CidFamily::AdobeJapan1
                        | CidFamily::AdobeGB1
                        | CidFamily::AdobeCNS1
                )
            });
            if cjk {
                return Some((Arc::clone(font), 0));
            }
        }
        stock(query)
    });
    settings
}

fn render_pdf_pages(
    pdf_bytes: Vec<u8>,
    ctx: RenderCtx<'_>,
) -> Result<(usize, Vec<RenderedPageEntry>), ScreenshotError> {
    let mut limits = Limits::DEFAULT;
    limits.source_bytes = MAX_SCREENSHOT_FILE_BYTES;
    limits.pages = MAX_PAGES;
    let doc = open_within(&Arc::new(pdf_bytes), ctx.deadline, &limits)
        .map_err(ScreenshotError::Runtime)?;
    let pages = doc.pages();
    let page_count = pages.len();
    let selected = resolve_requested_pages(ctx.pages, page_count)?;

    let cache = RenderCache::new();
    let settings = pdf_interpreter_settings(ctx.cjk_font);
    let render_settings = RenderSettings::default();
    let mut resources = Resources::default();
    let mut entries = Vec::with_capacity(selected.len());

    for page_no in selected {
        ctx.deadline.check().map_err(ScreenshotError::Runtime)?;
        let page = pages
            .get(page_no - 1)
            .ok_or_else(|| ScreenshotError::rt(format!("{page_no}쪽을 찾을 수 없습니다.")))?;
        let (width_pt, height_pt) = page.render_dimensions();
        if !(width_pt.is_finite() && height_pt.is_finite())
            || width_pt < MIN_PAGE_SIDE_PT
            || height_pt < MIN_PAGE_SIDE_PT
        {
            return Err(ScreenshotError::rt(format!(
                "{page_no}쪽 크기({width_pt}x{height_pt}pt)를 그릴 수 없습니다."
            )));
        }
        let raw_scale = ctx.dpi as f32 / 72.0;
        let (w_px, h_px) = clamp_pixel_dimensions(width_pt * raw_scale, height_pt * raw_scale)?;
        let transform = Affine::new([
            f64::from(w_px) / f64::from(width_pt),
            0.0,
            0.0,
            f64::from(h_px) / f64::from(height_pt),
            0.0,
            0.0,
        ]) * page.initial_transform(true).to_kurbo();
        let (w_u16, h_u16) = (w_px as u16, h_px as u16);
        let mut context = RenderContext::new(w_u16, h_u16);
        render_into(
            page,
            &cache,
            &settings,
            &render_settings,
            &mut context,
            transform,
        );
        context.flush();
        ctx.deadline.check().map_err(ScreenshotError::Runtime)?;
        let mut pixmap = Pixmap::new(w_u16, h_u16);
        context.render_with(
            &mut pixmap,
            &mut resources,
            RasterizerSettings {
                target_init: TargetInit::Clear(WHITE),
                ..Default::default()
            },
        );
        let png = pixmap
            .into_png()
            .map_err(|_| ScreenshotError::rt("PNG 인코딩에 실패했습니다."))?;
        entries.push(write_rendered_page(ctx.out_dir, page_no, w_px, h_px, &png)?);
    }
    Ok((page_count, entries))
}

#[inline]
fn is_formula_cell(st: Option<&SheetStyling>, r: u32, c: u32, raw: &str) -> bool {
    st.map_or_else(|| CellValueKind::infer(raw), |s| s.cell_kind(r, c, raw))
        == CellValueKind::Formula
}

fn evaluate_workbook_for_pdf(
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
) -> Vec<OdsUnpacked> {
    let has_formula = sheets.iter().enumerate().any(|(idx, (_, cells))| {
        let st = stylings.get(idx);
        cells
            .iter()
            .any(|(r, c, raw)| is_formula_cell(st, *r, *c, raw))
    });
    if !has_formula {
        return sheets.to_vec();
    }
    let mut wb = WorkbookEngine::new();
    let mut a1_pairs: Vec<(&str, String)> = Vec::new();
    for (idx, (name, cells)) in sheets.iter().enumerate() {
        if wb.try_add_sheet(name.as_str()).is_err() {
            return sheets.to_vec();
        }
        let st = stylings.get(idx);
        if let Some(s) = st {
            let _ = wb.set_tables(name, s.tables.clone());
            let (hidden, filtered) = s.hidden_row_runs();
            if let Some(sh) = wb.sheet_mut(name.as_str()) {
                sh.set_hidden_rows(hidden, filtered);
                sh.set_spill_merges(s.merges.clone());
            }
        }
        if let Some(sh) = wb.sheet_mut(name.as_str()) {
            for (r, c, raw) in cells {
                if let Ok(coord) = PackedCellCoordinate::new(*r, *c) {
                    let kind = st.and_then(|s| s.explicit_cell_kind(*r, *c, raw));
                    sh.set_cell_typed_coord(coord, raw.clone(), kind);
                    if is_formula_cell(st, *r, *c, raw) {
                        a1_pairs.push((name.as_str(), coord.to_a1()));
                    }
                }
            }
        }
    }
    let targets: Vec<WorkbookEvalTarget<'_>> = a1_pairs
        .iter()
        .map(|(sheet, expr)| WorkbookEvalTarget {
            sheet,
            expr: expr.as_str(),
            anchor: None,
        })
        .collect();
    let batch = wb.evaluate_targets_within(&targets, Duration::from_secs(5));
    let mut eval_iter = batch.values.iter().enumerate();
    sheets
        .iter()
        .enumerate()
        .map(|(idx, (name, cells))| {
            let st = stylings.get(idx);
            let out_cells = cells
                .iter()
                .filter_map(|(r, c, raw)| {
                    if !is_formula_cell(st, *r, *c, raw) {
                        return Some((*r, *c, raw.clone()));
                    }
                    PackedCellCoordinate::new(*r, *c).ok()?;
                    let display = match eval_iter.next() {
                        Some((ti, val)) if !batch.is_unreached(ti) => val.to_display_string(),
                        _ => EvaluatedCellValue::Empty.to_display_string(),
                    };
                    Some((*r, *c, display))
                })
                .collect();
            (name.clone(), out_cells)
        })
        .collect()
}

fn render_presentation(
    bytes: &[u8],
    hint: Option<&str>,
    ctx: RenderCtx<'_>,
) -> Result<(usize, Vec<RenderedPageEntry>), ScreenshotError> {
    let (decoded, _, slide_deck) = UniversalFormatConverter::decode_rich_full(bytes, hint)
        .map_err(|e| ScreenshotError::rt(e.to_string()))?;
    let deck = match (slide_deck, decoded.content) {
        (Some(d), _) => d,
        (None, DecodedContent::Slides(slides)) => {
            RichSlideDeck::from_legacy_slides(decoded.title, false, slides)
        }
        _ => return Err(ScreenshotError::rt("프레젠테이션 모델을 읽지 못했습니다.")),
    };
    let pdf_bytes =
        encode_slide_pdf(&deck, &deck.title).map_err(|e| ScreenshotError::rt(e.to_string()))?;
    render_pdf_pages(pdf_bytes, ctx)
}

fn render_workbook(
    bytes: &[u8],
    hint: Option<&str>,
    ctx: RenderCtx<'_>,
) -> Result<(usize, Vec<RenderedPageEntry>), ScreenshotError> {
    let (decoded, styling, _) = UniversalFormatConverter::decode_rich_full(bytes, hint)
        .map_err(|e| ScreenshotError::rt(e.to_string()))?;
    let DecodedContent::Workbook(sheets) = decoded.content else {
        return Err(ScreenshotError::rt("스프레드시트 모델을 읽지 못했습니다."));
    };
    let ordered_stylings: Vec<SheetStyling> = sheets
        .iter()
        .map(|(name, _)| {
            styling
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, s)| s.clone())
                .unwrap_or_default()
        })
        .collect();
    let computed = evaluate_workbook_for_pdf(&sheets, &ordered_stylings);
    let title = if decoded.title.trim().is_empty() {
        "시트"
    } else {
        decoded.title.as_str()
    };
    let pdf_bytes = encode_sheet_pdf(title, &computed, &ordered_stylings, &[])
        .map_err(|e| ScreenshotError::rt(e.to_string()))?;
    render_pdf_pages(pdf_bytes, ctx)
}

fn render_rich_doc(
    bytes: &[u8],
    hint: Option<&str>,
    ctx: RenderCtx<'_>,
) -> Result<(usize, Vec<RenderedPageEntry>), ScreenshotError> {
    let decoded = UniversalFormatConverter::decode_rich(bytes, hint)
        .map_err(|e| ScreenshotError::rt(e.to_string()))?;
    let DecodedContent::Document(rich) = decoded.content else {
        return Err(ScreenshotError::rt("문서 모델을 읽지 못했습니다."));
    };
    if let Ok(hwpx) = UniversalFormatConverter::encode_rich(&rich, DocumentFormat::Hwpx, None) {
        match render_rhwp_pages(&hwpx, ctx) {
            Ok(ok) => return Ok(ok),
            Err(ScreenshotError::Usage(msg)) => return Err(ScreenshotError::Usage(msg)),
            Err(ScreenshotError::Runtime(_)) => {}
        }
    }
    let (pdf_bytes, _) =
        UniversalFormatConverter::encode_rich_pdf(&rich, &PdfWriteOptions::default());
    render_pdf_pages(pdf_bytes, ctx)
}

fn run_cli(raw_args: &[OsString]) -> Result<String, ScreenshotError> {
    let args = parse_cli_args(raw_args)?;
    let bytes = read_bounded_input(&args.input_path)?;
    let canon_out = prepare_output_dir(&args.out_dir)?;
    let hint = args.input_path.file_name().and_then(|n| n.to_str());
    let format =
        FormatDetector::detect(&bytes, hint).map_err(|e| ScreenshotError::rt(e.to_string()))?;

    let deadline = Deadline::after(SCREENSHOT_DEADLINE);
    let font_dirs = screenshot_font_dirs();
    let fonts = shared_font_db(&font_dirs);
    let cjk_font = extract_cjk_font_bytes(&fonts);
    let ctx = RenderCtx {
        pages: args.pages.as_deref(),
        dpi: args.dpi,
        fonts: &fonts,
        cjk_font: cjk_font.as_deref(),
        out_dir: &canon_out,
        deadline: &deadline,
    };

    let (page_count, files) = match format {
        DocumentFormat::Hwp | DocumentFormat::Hwt | DocumentFormat::Hwpx | DocumentFormat::Hwtx => {
            render_rhwp_pages(&bytes, ctx)?
        }
        DocumentFormat::Pdf => render_pdf_pages(bytes, ctx)?,
        DocumentFormat::Pptx | DocumentFormat::Ppsx | DocumentFormat::Odp | DocumentFormat::Ppt => {
            render_presentation(&bytes, hint, ctx)?
        }
        DocumentFormat::Xlsx
        | DocumentFormat::Xls
        | DocumentFormat::Ods
        | DocumentFormat::Csv
        | DocumentFormat::Tsv => render_workbook(&bytes, hint, ctx)?,
        DocumentFormat::Docx
        | DocumentFormat::Doc
        | DocumentFormat::Odt
        | DocumentFormat::Rtf
        | DocumentFormat::Markdown
        | DocumentFormat::Txt
        | DocumentFormat::Pages => render_rich_doc(&bytes, hint, ctx)?,
    };

    let summary = ScreenshotSummary {
        status: "ok",
        format: format.extension(),
        page_count,
        dpi: args.dpi,
        rendered_count: files.len(),
        files,
    };
    serde_json::to_string(&summary)
        .map_err(|e| ScreenshotError::rt(format!("결과 JSON 직렬화 실패: {e}")))
}
