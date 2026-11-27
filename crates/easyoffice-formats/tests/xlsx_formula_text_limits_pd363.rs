//! PD-361: formula spelling conversion has a length cap and a nesting cap, takes linear time, and
//! needs no recursion. Input files are untrusted (PD-262).

use easyoffice_formats::{from_file_formula, to_file_formula};
use std::time::{Duration, Instant};

/// A budget far above the linear cost and far below the quadratic one.
const TIME_LIMIT: Duration = Duration::from_secs(2);

/// Runs on a small stack, the way a worker thread of a host app does. A recursive conversion
/// aborts the whole process here.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let value = f();
    (value, start.elapsed())
}

#[test]
fn nested_single_calls_keep_their_text_and_finish_fast() {
    let n = 30_000;
    let file = format!("{}A1{}", "_xlfn.SINGLE(".repeat(n), ")".repeat(n));
    let expected = file.clone();
    let (out, took) = timed(move || on_small_stack(move || from_file_formula(&file)));
    assert_eq!(out, expected, "over the cap the text is kept as it is");
    assert!(took < TIME_LIMIT, "{took:?}");
}

/// Under the length cap, so only the nesting cap stops these.
#[test]
fn deep_nesting_inside_the_length_cap_is_kept_and_finishes_fast() {
    let calls = format!("{}A1{}", "_xlfn.SINGLE(".repeat(500), ")".repeat(500));
    let ats = "@".repeat(8_000);
    let at_parens = "@(".repeat(4_000);
    let singles = "SINGLE(".repeat(1_100);
    for (shape, from_file) in [
        (calls, true),
        (ats, false),
        (at_parens, false),
        (singles, true),
    ] {
        assert!(shape.chars().count() <= 8_192);
        let expected = shape.clone();
        let (out, took) = timed(move || {
            on_small_stack(move || {
                if from_file {
                    from_file_formula(&shape)
                } else {
                    to_file_formula(&shape)
                }
            })
        });
        assert_eq!(out, expected);
        assert!(took < TIME_LIMIT, "{took:?}");
    }
}

#[test]
fn nested_single_calls_just_over_the_depth_cap_are_kept() {
    let n = 65;
    let file = format!("{}A1{}", "_xlfn.SINGLE(".repeat(n), ")".repeat(n));
    assert_eq!(from_file_formula(&file), file);
    let page = format!("{}A1", "@".repeat(n));
    assert_eq!(to_file_formula(&page), page);
}

#[test]
fn nested_single_calls_at_the_depth_cap_still_convert() {
    let n = 64;
    let file = format!("{}A1{}", "_xlfn.SINGLE(".repeat(n), ")".repeat(n));
    let page = from_file_formula(&file);
    assert_eq!(page, format!("{}A1", "@".repeat(n)));
    assert_eq!(to_file_formula(&page), file);
}

#[test]
fn a_long_run_of_at_signs_finishes_fast_and_is_kept() {
    let page = "@".repeat(30_000);
    let expected = page.clone();
    let (out, took) = timed(move || on_small_stack(move || to_file_formula(&page)));
    assert_eq!(out, expected);
    assert!(took < TIME_LIMIT, "{took:?}");
}

#[test]
fn at_signs_before_open_parentheses_finish_fast_and_are_kept() {
    let page = "@(".repeat(8_000);
    let expected = page.clone();
    let (out, took) = timed(move || on_small_stack(move || to_file_formula(&page)));
    assert_eq!(out, expected);
    assert!(took < TIME_LIMIT, "{took:?}");
}

#[test]
fn a_formula_over_the_length_limit_is_kept_in_both_directions() {
    // 8,192 characters is the longest formula the spreadsheet format allows.
    let long = format!("SUM({})", "A1,".repeat(3_000));
    assert!(long.chars().count() > 8_192);
    assert_eq!(
        to_file_formula(&format!("FILTER({long})")),
        format!("FILTER({long})")
    );
    let file = format!("_xlfn.IFS({})", "A1,".repeat(3_000));
    assert_eq!(from_file_formula(&file), file);
}

#[test]
fn a_formula_at_the_length_limit_still_converts() {
    let body = "A1,".repeat(2_700);
    let page = format!("FILTER({body}B1)");
    assert!(page.chars().count() <= 8_192);
    assert_eq!(
        to_file_formula(&page),
        format!("_xlfn._xlws.FILTER({body}B1)")
    );
}

#[test]
fn many_single_calls_side_by_side_stay_linear() {
    // Sibling calls do not nest, so every one converts (the file spelling is the longer one).
    let page = "@A1+".repeat(400);
    let (file, took) = timed(|| to_file_formula(&page));
    assert!(took < TIME_LIMIT, "{took:?}");
    assert_eq!(file, "_xlfn.SINGLE(A1)+".repeat(400));
    let (back, took) = timed(|| from_file_formula(&file));
    assert!(took < TIME_LIMIT, "{took:?}");
    assert_eq!(back, page);
}

#[test]
fn unbalanced_adversarial_shapes_finish_fast() {
    let shapes = [
        "SINGLE(".repeat(1_100),
        "_xlfn.SINGLE(a,".repeat(500),
        "@ ".repeat(4_000),
        "@A1:".repeat(2_000),
        "(".repeat(8_000),
        "SINGLE(( ".repeat(800),
        "_xlfn.ANCHORARRAY(".repeat(400),
    ];
    for shape in shapes {
        let s = shape.clone();
        let (_, took) = timed(move || on_small_stack(move || to_file_formula(&s)));
        assert!(took < TIME_LIMIT, "to_file {took:?} {}", &shape[..12]);
        let s = shape.clone();
        let (_, took) = timed(move || on_small_stack(move || from_file_formula(&s)));
        assert!(took < TIME_LIMIT, "from_file {took:?} {}", &shape[..12]);
    }
}
