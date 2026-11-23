//! 참조 구현: IronCalc test cases of the reference engine (`base/src/test/`, commit
//! f93662006b8fb1c401dc870bbda26c5d60ac6037; MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH,
//! 2023-present IronCalc contributors, 2026-present IronCalc GmbH) that this engine gives the same
//! result for. Each module is one reference file (the module name is its path with `/` as `_`);
//! the cases whose result differs are left out and listed in `docs/sheet-function-parity.md`.
//! The cases run on the shared `dynamic_support` model.
#![allow(clippy::all, dead_code, unused)]

#[path = "../dynamic_support/mod.rs"]
mod dynamic_support;

mod test {
    pub mod util {
        pub use crate::dynamic_support::new_empty_model;
        pub fn new_empty_model_ref_1900() -> crate::dynamic_support::Model {
            new_empty_model()
        }
    }
}

mod dynamic_evaluation_test_evaluation_order;
mod dynamic_evaluation_test_randarray;
mod dynamic_evaluation_test_wrong_order;
mod engineering_test_bit_operations;
mod engineering_test_complex;
mod engineering_test_misc;
mod engineering_test_number_basis;
mod logical_test_fn_if_array;
mod logical_test_fn_iferror_array;
mod logical_test_fn_not_array;
mod lookup_and_reference_test_fn_address_areas;
mod lookup_and_reference_test_fn_choosecols_chooserows;
mod lookup_and_reference_test_fn_expand;
mod lookup_and_reference_test_fn_hlookup;
mod lookup_and_reference_test_fn_hstack_vstack;
mod lookup_and_reference_test_fn_index;
mod lookup_and_reference_test_fn_lookup;
mod lookup_and_reference_test_fn_match;
mod lookup_and_reference_test_fn_rows_columns;
mod lookup_and_reference_test_fn_trimrange;
mod lookup_and_reference_test_fn_vlookup;
mod lookup_and_reference_test_fn_wrapcols_wraprows;
mod mathematical_test_mdeterm;
mod mathematical_test_minverse;
mod mathematical_test_multinomial;
mod mathematical_test_munit;
mod mathematical_test_seriessum;
mod mathematical_test_sumproduct;
mod statistical_test_fn_binom;
mod statistical_test_fn_skew_p;
mod test_arrays;
mod test_base_decimal;
mod test_ceiling_floor;
mod test_datevalue_timevalue_spaces;
mod test_days360_month_end;
mod test_escape_quotes;
mod test_even_odd;
mod test_floor;
mod test_fn_accrint;
mod test_fn_accrintm;
mod test_fn_bond_functions;
mod test_fn_choose;
mod test_fn_coupon_dates;
mod test_fn_day;
mod test_fn_drop_take;
mod test_fn_exact;
mod test_fn_financial_simple;
mod test_fn_if;
mod test_fn_isomitted;
mod test_fn_let;
mod test_fn_map_reduce;
mod test_fn_minute_second_rounding;
mod test_fn_oddfprice;
mod test_fn_offset;
mod test_fn_or_xor;
mod test_fn_randarray;
mod test_fn_rept;
mod test_fn_round;
mod test_fn_scan;
mod test_fn_sequence;
mod test_fn_tocol_torow;
mod test_fn_transpose;
mod test_forecast;
mod test_format_case;
mod test_geomean;
mod test_implicit_intersection;
mod test_indirect_r1c1;
mod test_issue_623;
mod test_mround_trunc_int;
mod test_networkdays;
mod test_range_evaluation;
mod test_spill_functions;
mod test_statistical_functions;
mod test_trigonometric;
mod test_weekday_arg_order;
mod test_weekday_return_types;
mod test_weekday_weeknum_isoweeknum;
mod test_weeknum_return_types;
mod test_workday_workdayintl;
mod text_functions_byte_variants;
mod text_functions_fn_arraytotext;
mod text_functions_fn_char_code;
mod text_functions_fn_clean;
mod text_functions_fn_regex;
mod text_functions_fn_textsplit;
mod text_functions_fn_unicode;
