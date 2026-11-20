// 참조 구현 (reference implementation): exponential smoothing forecast, ported from the spreadsheet
// program LibreOffice Calc (https://github.com/LibreOffice/core).
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Origin (MPL-2.0, QA decision 2026-11-21, PD-272; see THIRD_PARTY_NOTICES.md):
//   repository  https://github.com/LibreOffice/core, commit 2b66365f1b81b7a89bdc1d93788e4c96073fa513
//   `sc/source/core/tool/interpr8.cxx` lines 25-1172: `DataPoint`, `lcl_SortByX` and the class
//     `ScETSForecastCalculation` with all its methods (1,148 lines of C++)
//   `sc/inc/kahan.hxx` lines 46-278: `KahanSum` (the sum the calculation uses)
// Copyright: the sources carry no copyright line. The history names Winfried Donkers as the author of
// the forecast functions (2016) and other contributors of the project for later changes.
//
// This is the only file with code under the MPL. It is to be replaced by a clean-room
// implementation (TODO.md). The engine reaches it only through `functions_forecast_ets_args.rs`,
// which reads the arguments (the original's `ScInterpreter::ScForecast_Ets`), and the tests of that
// file run this calculation over the data of the original's tests.
//
// Changes from the original, each marked `CHANGE`: snake case names, slices for matrices, a context
// of services in place of the interpreter; limits that fail closed (series length, step and byte
// budget, prediction steps); the random numbers come from the caller; the average of equal timeline
// values is a real average; the gap fill of a month timeline uses the month numbers it holds; the
// off-grid interpolation of the prediction interval is a fraction of a step; a series with no
// seasonality takes the closed form interval; the all-zero period average is an error only for the
// multiplicative model; the three searches of a coefficient are one function; `KahanSum` is a plain
// Neumaier sum.

/// Largest series (after gaps are filled) the calculation takes. CHANGE: a limit of this port.
const MAX_DATA_POINTS: usize = 32_768;
/// Largest number of forecast steps whose prediction interval is simulated: the scenario table has
/// `steps * 1000` numbers (32 MiB at the limit). CHANGE: a limit of this port.
const MAX_PI_STEPS: usize = 4_096;
/// Farthest forecast, in steps from the end of the series: the steps are counted in a `f64`
/// and a whole number of them is exact below this. CHANGE: a limit of this port.
const MAX_FORECAST_STEPS: f64 = 9_007_199_254_740_992.0;
/// Numbers of the series one evaluation step of the budget stands for. CHANGE: a limit of this port.
const STEP_VALUES: usize = 64;

/// The error values the calculation gives (`FormulaError` of the original, the ones used here).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormulaError {
    NoError,
    IllegalFPOperation,
    NoValue,
    DivisionByZero,
    UnknownState,
    IllegalArgument,
    /// The work or memory budget is used up, or a limit of this port is exceeded. CHANGE: new.
    Aborted,
}

/// The kind of result (`ScETSType`). The multiplicative kinds are the original's; the engine's
/// functions use the additive ones and the tests use both.
#[allow(clippy::enum_variant_names, dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScETSType {
    EtsAdd,
    EtsMult,
    EtsPIAdd,
    EtsPIMult,
    EtsStatAdd,
    EtsStatMult,
    EtsSeason,
}

/// What the calculation takes from the program around it (`ScInterpreterContext` of the original
/// gave the null date). CHANGE: services in place of the date format object.
pub(crate) struct ScInterpreterContext<'a> {
    /// Year, month, day of a day serial of the date system in use.
    pub civil_from_serial: fn(i64) -> (i64, i64, i64),
    /// The inverse of the standard normal distribution (`ScInterpreter::gaussinv`).
    pub gaussinv: fn(f64) -> f64,
    /// The percentile of a list with linear interpolation (`ScInterpreter::GetPercentile`).
    pub percentile: fn(&mut [f64], f64) -> f64,
    /// A uniform number in [0, 1) (`comphelper::rng::uniform_real_distribution`).
    pub rand: &'a mut dyn FnMut() -> f64,
    /// Charges the evaluation budget; `false` when it or the deadline is used up.
    pub budget: &'a mut dyn FnMut(Budget) -> bool,
}

/// A charge to the evaluation budget.
#[derive(Clone, Copy)]
pub(crate) enum Budget {
    /// Evaluation steps.
    Steps(usize),
    /// Bytes of the array budget.
    Bytes(usize),
}

#[derive(Clone, Copy, Debug)]
struct DataPoint {
    x: f64,
    y: f64,
}

/// Neumaier sum (`KahanSum`). CHANGE: the original also holds back the last summand to cancel an
/// almost equal opposite one.
#[derive(Clone, Copy, Default)]
struct KahanSum {
    sum: f64,
    error: f64,
}

impl KahanSum {
    fn new(x_0: f64) -> Self {
        Self {
            sum: x_0,
            error: 0.0,
        }
    }

    fn add(&mut self, x_i: f64) {
        let t = self.sum + x_i;
        if self.sum.abs() >= x_i.abs() {
            self.error += (self.sum - t) + x_i;
        } else {
            self.error += (x_i - t) + self.sum;
        }
        self.sum = t;
    }

    fn div_assign(&mut self, f_divides: f64) {
        *self = Self::new(self.get() / f_divides);
    }

    fn get(&self) -> f64 {
        self.sum + self.error
    }
}

impl std::ops::AddAssign<f64> for KahanSum {
    fn add_assign(&mut self, x: f64) {
        self.add(x);
    }
}

impl std::ops::SubAssign<f64> for KahanSum {
    fn sub_assign(&mut self, x: f64) {
        self.add(-x);
    }
}

#[derive(Clone, Copy)]
enum Coefficient {
    Alpha,
    Beta,
    Gamma,
}

/// Smallest change of alpha, beta, gamma the search looks at.
const CF_MIN_ABC_RESOLUTION: f64 = 0.001;
/// Number of scenarios of the prediction interval.
const CN_SCENARIOS: usize = 1000;

/// Holt-Winters triple exponential smoothing and, with a seasonal period of 0, the double form
/// (`ScETSForecastCalculation`).
pub(crate) struct ScETSForecastCalculation<'a> {
    mr_context: ScInterpreterContext<'a>,
    ma_range: Vec<DataPoint>,
    mp_base: Vec<f64>,
    mp_trend: Vec<f64>,
    mp_per_idx: Vec<f64>,
    mp_forecast: Vec<f64>,
    mn_smpl_in_prd: usize,
    mf_step_size: f64,
    mf_alpha: f64,
    mf_beta: f64,
    mf_gamma: f64,
    mn_count: usize,
    mb_initialised: bool,
    mn_month_day: i64,
    mf_mae: f64,
    mf_mase: f64,
    mf_mse: f64,
    mf_rmse: f64,
    mf_smape: f64,
    mn_error_value: FormulaError,
    b_additive: bool,
    b_eds: bool,
}

/// The values of one run of equal timeline values combined by an aggregation method.
fn aggregate(run: &[f64], n_aggregation: i32) -> Option<f64> {
    let sum = || run.iter().sum::<f64>();
    match n_aggregation {
        // AVERAGE (default). CHANGE: the original keeps the first value of the run.
        1 => Some(sum() / run.len() as f64),
        // SUM
        7 => Some(sum()),
        // COUNT, COUNTA (same as COUNT as there are no non-numeric Y-values)
        2 | 3 => Some(run.len() as f64),
        // MAX
        4 => Some(run.iter().copied().fold(run[0], f64::max)),
        // MEDIAN
        5 => {
            let mut a_tmp = run.to_vec();
            a_tmp.sort_by(f64::total_cmp);
            let mid = a_tmp.len() / 2;
            Some(if a_tmp.len() % 2 == 1 {
                a_tmp[mid]
            } else {
                (a_tmp[mid] + a_tmp[mid - 1]) / 2.0
            })
        }
        // MIN
        6 => Some(run.iter().copied().fold(run[0], f64::min)),
        _ => None,
    }
}

impl<'a> ScETSForecastCalculation<'a> {
    pub(crate) fn new(n_size: usize, r_context: ScInterpreterContext<'a>) -> Self {
        Self {
            mr_context: r_context,
            ma_range: Vec::new(),
            mp_base: Vec::new(),
            mp_trend: Vec::new(),
            mp_per_idx: Vec::new(),
            mp_forecast: Vec::new(),
            mn_smpl_in_prd: 0,
            mf_step_size: 0.0,
            mf_alpha: 0.0,
            mf_beta: 0.0,
            mf_gamma: 0.0,
            mn_count: n_size,
            mb_initialised: false,
            mn_month_day: 0,
            mf_mae: 0.0,
            mf_mase: 0.0,
            mf_mse: 0.0,
            mf_rmse: 0.0,
            mf_smape: 0.0,
            mn_error_value: FormulaError::NoError,
            b_additive: false,
            b_eds: false,
        }
    }

    pub(crate) fn get_error(&self) -> FormulaError {
        self.mn_error_value
    }

    /// The series has no seasonality (a period of 0 or a detected period of 1).
    pub(crate) fn b_eds(&self) -> bool {
        self.b_eds
    }

    /// Year, month, day of the date `x` days after the null date (the original adds
    /// `static_cast<sal_Int32>(x)`).
    fn date_of(&self, x: f64) -> (i64, i64, i64) {
        (self.mr_context.civil_from_serial)(i64::from(x as i32))
    }

    fn fail(&mut self, error: FormulaError) -> bool {
        self.mn_error_value = error;
        false
    }

    /// The step budget of `values` numbers of the series; the calculation stops when it is used up.
    fn charge(&mut self, values: usize) -> bool {
        (self.mr_context.budget)(Budget::Steps(values / STEP_VALUES + 1))
            || self.fail(FormulaError::Aborted)
    }

    /// `r_mat_x` is the timeline, `r_mat_y` the values, `r_t_mat` the targets when the result needs
    /// them (only the first one is checked, as in the original).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn preprocess_data_range(
        &mut self,
        r_mat_x: &[f64],
        r_mat_y: &[f64],
        n_smpl_in_prd: usize,
        b_data_completion: bool,
        n_aggregation: i32,
        r_t_mat: Option<&[f64]>,
        e_ets_type: ScETSType,
    ) -> bool {
        use ScETSType::{EtsAdd, EtsPIAdd, EtsPIMult, EtsStatAdd};
        self.b_eds = n_smpl_in_prd == 0;
        self.b_additive = matches!(e_ets_type, EtsAdd | EtsPIAdd | EtsStatAdd);

        // CHANGE: limits of this port
        if self.mn_count != r_mat_x.len() || self.mn_count != r_mat_y.len() {
            return self.fail(FormulaError::IllegalArgument);
        }
        if self.mn_count > MAX_DATA_POINTS {
            return self.fail(FormulaError::Aborted);
        }
        if self.mn_count < 2 {
            return self.fail(FormulaError::NoValue);
        }

        // ma_range needs to be sorted by X
        let points = r_mat_x.iter().zip(r_mat_y);
        self.ma_range
            .extend(points.map(|(&x, &y)| DataPoint { x, y }));
        self.ma_range.sort_by(|a, b| a.x.total_cmp(&b.x));

        if let Some(f_target) = r_t_mat.and_then(|t| t.first().copied()) {
            // target cannot be less than start of X-range (before the end of it for a prediction interval)
            let bound = if matches!(e_ets_type, EtsPIAdd | EtsPIMult) {
                self.ma_range[self.mn_count - 1].x
            } else {
                self.ma_range[0].x
            };
            if f_target < bound {
                return self.fail(FormulaError::IllegalFPOperation);
            }
        }

        // Month intervals don't have exact stepsize, so first detect if month interval is used.
        // Method: assume there is an month interval and verify.
        // If month interval is used, replace ma_range.x with month values for ease of calculations.
        let a_date = self.date_of(self.ma_range[0].x);
        let mut month_day = a_date.2;
        for p in &self.ma_range[1..] {
            let a_date1 = self.date_of(p.x);
            if a_date != a_date1 && a_date1.2 != month_day {
                month_day = 0;
                break;
            }
        }
        self.mn_month_day = month_day;

        self.mf_step_size = f64::MAX;
        if self.mn_month_day != 0 {
            for i in 0..self.mn_count {
                let (year, month, _) = self.date_of(self.ma_range[i].x);
                self.ma_range[i].x = (year * 12 + month) as f64;
            }
        }
        let mut i = 1;
        while i < self.mn_count {
            let mut f_step = self.ma_range[i].x - self.ma_range[i - 1].x;
            if f_step == 0.0 {
                if n_aggregation == 0 {
                    // identical X-values are not allowed
                    return self.fail(FormulaError::NoValue);
                }
                // The run of equal X-values that starts before i. CHANGE: one drain replaces the
                // erase of one element at a time (a work limit).
                let run_x = self.ma_range[i - 1].x;
                let run_len = self.ma_range[i..]
                    .iter()
                    .take_while(|p| p.x == run_x)
                    .count();
                let run: Vec<f64> = self.ma_range[i - 1..=i + run_len - 1]
                    .iter()
                    .map(|p| p.y)
                    .collect();
                if let Some(y) = aggregate(&run, n_aggregation) {
                    self.ma_range.drain(i..i + run_len);
                    self.mn_count = self.ma_range.len();
                    self.ma_range[i - 1].y = y;
                }
                f_step = if i + 1 < self.mn_count {
                    self.ma_range[i].x - self.ma_range[i - 1].x
                } else {
                    self.mf_step_size
                };
            }
            if f_step > 0.0 && f_step < self.mf_step_size {
                self.mf_step_size = f_step;
            }
            i += 1;
        }
        if self.mn_count < 2 {
            return self.fail(FormulaError::NoValue);
        }

        // step must be constant (or gap multiple of step)
        let mut b_has_gap = false;
        for w in self.ma_range.windows(2) {
            let f_step = w[1].x - w[0].x;
            if f_step != self.mf_step_size {
                if f_step % self.mf_step_size != 0.0 {
                    // step not constant nor multiple of mf_step_size in case of gaps
                    return self.fail(FormulaError::NoValue);
                }
                b_has_gap = true;
                break;
            }
        }

        // fill gaps with values depending on b_data_completion
        if b_has_gap && !self.fill_gaps(b_data_completion) {
            return false;
        }

        if n_smpl_in_prd != 1 {
            self.mn_smpl_in_prd = n_smpl_in_prd;
        } else {
            self.mn_smpl_in_prd = self.calc_period_len();
            if self.mn_error_value != FormulaError::NoError {
                return false;
            }
            if self.mn_smpl_in_prd == 1 {
                self.b_eds = true; // period length 1 means no periodic data: EDS suffices
            }
        }

        self.init_data()
    }

    /// The block of `PreprocessDataRange` that inserts the missing points. CHANGE: it builds the
    /// new list in one pass (the original inserts into the middle of the list, a quadratic cost),
    /// and a month timeline measures the distance in the month numbers it holds (the original read
    /// them again as day serials).
    fn fill_gaps(&mut self, b_data_completion: bool) -> bool {
        let mut n_missing_x_count = 0usize;
        let f_original_count = self.mn_count as f64;
        let mut filled = vec![self.ma_range[0]];
        for w in self.ma_range.windows(2) {
            let (prev, cur) = (w[0], w[1]);
            if cur.x - prev.x > self.mf_step_size {
                // gap, insert missing data points
                let f_y_gap = (cur.y + prev.y) / 2.0;
                let mut f_x_gap = KahanSum::new(prev.x + self.mf_step_size);
                while f_x_gap.get() < cur.x {
                    let y = if b_data_completion { f_y_gap } else { 0.0 };
                    filled.push(DataPoint {
                        x: f_x_gap.get(),
                        y,
                    });
                    n_missing_x_count += 1;
                    if n_missing_x_count as f64 / f_original_count > 0.3 {
                        // maximum of 30% missing points exceeded
                        return self.fail(FormulaError::NoValue);
                    }
                    if filled.len() >= MAX_DATA_POINTS {
                        return self.fail(FormulaError::Aborted);
                    }
                    f_x_gap += self.mf_step_size;
                }
            }
            filled.push(cur);
        }
        self.mn_count = filled.len();
        self.ma_range = filled;
        true
    }

    fn init_data(&mut self) -> bool {
        // give various vectors size and initial value
        self.mp_base = vec![0.0; self.mn_count];
        self.mp_trend = vec![0.0; self.mn_count];
        if !self.b_eds {
            self.mp_per_idx = vec![0.0; self.mn_count];
        }
        self.mp_forecast = vec![0.0; self.mn_count];
        self.mp_forecast[0] = self.ma_range[0].y;

        if self.prefill_trend_data() && self.prefill_per_idx() {
            self.prefill_base_data();
            return true;
        }
        false
    }

    fn prefill_trend_data(&mut self) -> bool {
        let (n, smpl) = (self.mn_count, self.mn_smpl_in_prd);
        if self.b_eds {
            self.mp_trend[0] = (self.ma_range[n - 1].y - self.ma_range[0].y) / (n - 1) as f64;
            return true;
        }
        // we need at least 2 periods in the data range
        if n < 2 * smpl {
            return self.fail(FormulaError::NoValue);
        }
        let mut f_sum = KahanSum::new(0.0);
        for i in 0..smpl {
            f_sum += self.ma_range[i + smpl].y;
            f_sum -= self.ma_range[i].y;
        }
        self.mp_trend[0] = f_sum.get() / (smpl * smpl) as f64;
        true
    }

    fn prefill_per_idx(&mut self) -> bool {
        if self.b_eds {
            return true;
        }
        // use as many complete periods as available
        let smpl = self.mn_smpl_in_prd;
        if smpl == 0 {
            // should never happen; if mn_smpl_in_prd equals 0, b_eds is true
            return self.fail(FormulaError::UnknownState);
        }
        let n_periods = self.mn_count / smpl;
        let mut a_period_average = vec![KahanSum::new(0.0); n_periods];
        for (i, average) in a_period_average.iter_mut().enumerate() {
            for p in &self.ma_range[i * smpl..(i + 1) * smpl] {
                *average += p.y;
            }
            average.div_assign(smpl as f64);
            // CHANGE: the original also refuses the additive model here, which does not divide by it
            if !self.b_additive && average.get() == 0.0 {
                // average of 0 will cause divide by zero error, quitting calculation
                return self.fail(FormulaError::DivisionByZero);
            }
        }

        for j in 0..smpl {
            let mut f_i = KahanSum::new(0.0);
            for (i, average) in a_period_average.iter().enumerate() {
                // adjust average value for position within period
                let level =
                    average.get() + (j as f64 - 0.5 * (smpl as f64 - 1.0)) * self.mp_trend[0];
                let y = self.ma_range[i * smpl + j].y;
                f_i += if self.b_additive {
                    y - level
                } else {
                    y / level
                };
            }
            self.mp_per_idx[j] = f_i.get() / n_periods as f64;
        }
        if smpl < self.mn_count {
            self.mp_per_idx[smpl] = 0.0;
        }
        true
    }

    fn prefill_base_data(&mut self) {
        let y = self.ma_range[0].y;
        self.mp_base[0] = if self.b_eds {
            y
        } else {
            y / self.mp_per_idx[0]
        };
    }

    fn init_calc(&mut self) {
        if !self.mb_initialised {
            self.calc_alpha_beta_gamma();

            self.mb_initialised = true;
            self.calc_accuracy_indicators();
        }
    }

    fn calc_accuracy_indicators(&mut self) {
        let mut f_sum_abs_err = KahanSum::new(0.0);
        let mut f_sum_divisor = KahanSum::new(0.0);
        let mut f_sum_err_sq = KahanSum::new(0.0);
        let mut f_sum_abs_perc_err = KahanSum::new(0.0);

        for i in 1..self.mn_count {
            let (f_fc, y) = (self.mp_forecast[i], self.ma_range[i].y);
            let f_error = f_fc - y;
            f_sum_abs_err += f_error.abs();
            f_sum_err_sq += f_error * f_error;
            f_sum_abs_perc_err += f_error.abs() / (f_fc.abs() + y.abs());
        }

        for w in self.ma_range.windows(2).skip(1) {
            f_sum_divisor += (w[1].y - w[0].y).abs();
        }

        let n_calc_count = (self.mn_count - 1) as f64;
        let abs_err = f_sum_abs_err.get();
        self.mf_mae = abs_err / n_calc_count;
        self.mf_mase = abs_err / (n_calc_count * f_sum_divisor.get() / (n_calc_count - 1.0));
        self.mf_mse = f_sum_err_sq.get() / n_calc_count;
        self.mf_rmse = self.mf_mse.sqrt();
        self.mf_smape = f_sum_abs_perc_err.get() * 2.0 / n_calc_count;
    }

    /// `calc_period_len()` calculates the most likely length of a period.
    ///
    /// Method used: for all possible values (between mn_count/2 and 2) compare for each (sample-previous
    /// sample) with next period and calculate mean error. Use as much samples as possible for each
    /// period length and the most recent samples. Return the period length with the lowest mean error.
    fn calc_period_len(&mut self) -> usize {
        let n = self.mn_count;
        let mut n_best_val = n;
        let mut f_best_me = f64::MAX;

        for n_period_len in (1..=n / 2).rev() {
            if !self.charge(n) {
                return n_best_val;
            }
            let mut f_mean_error = KahanSum::new(0.0);
            let n_periods = n / n_period_len;
            let n_start = n - (n_periods * n_period_len) + 1;
            for i in n_start..(n - n_period_len) {
                let (r, s) = (&self.ma_range, n_period_len);
                f_mean_error += ((r[i].y - r[i - 1].y) - (r[s + i].y - r[s + i - 1].y)).abs();
            }
            let f_mean_error_get =
                f_mean_error.get() / ((((n_periods - 1) * n_period_len) as f64) - 1.0);

            if f_mean_error_get <= f_best_me || f_mean_error_get == 0.0 {
                n_best_val = n_period_len;
                f_best_me = f_mean_error_get;
            }
        }
        n_best_val
    }

    fn calc_alpha_beta_gamma(&mut self) {
        if self.b_eds {
            self.mf_beta = 0.0; // beta is not used with EDS
        }
        self.search(Coefficient::Alpha);
    }

    fn calc_beta_gamma(&mut self) {
        self.search(Coefficient::Beta);
    }

    fn calc_gamma(&mut self) {
        self.search(Coefficient::Gamma);
    }

    /// Sets a coefficient, fits the ones that depend on it (alpha: beta and gamma, or gamma alone
    /// with EDS; beta: gamma) and refills the series.
    /// CHANGE: the original has this search three times, once per coefficient (`CalcAlphaBetaGamma`,
    /// `CalcBetaGamma`, `CalcGamma`), the text differing in the coefficient and in this fit.
    fn fit(&mut self, which: Coefficient, value: f64) {
        match which {
            Coefficient::Alpha => {
                self.mf_alpha = value;
                if self.b_eds {
                    self.calc_gamma();
                } else {
                    self.calc_beta_gamma();
                }
            }
            Coefficient::Beta => {
                self.mf_beta = value;
                self.calc_gamma();
            }
            Coefficient::Gamma => self.mf_gamma = value,
        }
        self.refill();
    }

    /// Bisection on one coefficient in [0, 1] down to `CF_MIN_ABC_RESOLUTION` for the least MSE.
    fn search(&mut self, which: Coefficient) {
        let mut f0 = 0.0;
        self.fit(which, f0);
        let mut f_e0 = self.mf_mse;

        let mut f2 = 1.0;
        self.fit(which, f2);
        let mut f_e2 = self.mf_mse;

        let mut f1 = 0.5;
        self.fit(which, f1);

        if f_e0 == self.mf_mse && self.mf_mse == f_e2 {
            self.fit(which, 0.0);
            return;
        }
        while (f2 - f1) > CF_MIN_ABC_RESOLUTION {
            if f_e2 > f_e0 {
                f2 = f1;
                f_e2 = self.mf_mse;
                f1 = (f0 + f1) / 2.0;
            } else {
                f0 = f1;
                f_e0 = self.mf_mse;
                f1 = (f1 + f2) / 2.0;
            }
            self.fit(which, f1);
        }
        if f_e2 > f_e0 {
            if f_e0 < self.mf_mse {
                self.fit(which, f0);
            }
        } else if f_e2 < self.mf_mse {
            self.fit(which, f2);
        }
    }

    fn refill(&mut self) {
        // refill mp_base, mp_trend, mp_per_idx and mp_forecast with values
        // using the calculated mf_alpha, (mf_beta), mf_gamma
        // forecast 1 step ahead
        // CHANGE: each pass is charged to the step budget and the pass stops when it is used up
        if self.mn_error_value != FormulaError::NoError || !self.charge(self.mn_count) {
            return;
        }
        let (a, b, g) = (self.mf_alpha, self.mf_beta, self.mf_gamma);
        let (smpl, additive) = (self.mn_smpl_in_prd, self.b_additive);
        for i in 1..self.mn_count {
            let y = self.ma_range[i].y;
            let level = self.mp_base[i - 1] + self.mp_trend[i - 1];
            if self.b_eds {
                self.mp_base[i] = a * y + (1.0 - a) * level;
                self.mp_forecast[i] = level;
            } else {
                let wraps = if additive { i > smpl } else { i >= smpl };
                let n_idx = if wraps { i - smpl } else { i };
                let idx = self.mp_per_idx[n_idx];
                if additive {
                    self.mp_base[i] = a * (y - idx) + (1.0 - a) * level;
                    self.mp_per_idx[i] = b * (y - self.mp_base[i]) + (1.0 - b) * idx;
                    // the index of this step, as updated when it is the one just written
                    self.mp_forecast[i] = level + self.mp_per_idx[n_idx];
                } else {
                    self.mp_base[i] = a * (y / idx) + (1.0 - a) * level;
                    self.mp_per_idx[i] = b * (y / self.mp_base[i]) + (1.0 - b) * idx;
                    self.mp_forecast[i] = level * self.mp_per_idx[n_idx];
                }
            }
            self.mp_trend[i] =
                g * (self.mp_base[i] - self.mp_base[i - 1]) + (1.0 - g) * self.mp_trend[i - 1];
        }
        self.calc_accuracy_indicators();
    }

    fn convert_x_to_months(&self, x: f64) -> f64 {
        let (year, month, day) = self.date_of(x);
        let f_month_length = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31.0,
            2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29.0,
            2 => 28.0,
            _ => 30.0,
        };
        12.0 * year as f64 + month as f64 + (day - self.mn_month_day) as f64 / f_month_length
    }

    /// The forecast `n` whole steps after the end of the series (`NaN` when the series has no
    /// seasonal index for it).
    fn forecast_after(&self, n: usize) -> f64 {
        let (last, smpl) = (self.mn_count - 1, self.mn_smpl_in_prd);
        let level = self.mp_base[last] + n as f64 * self.mp_trend[last];
        if self.b_eds {
            return level;
        }
        let idx = (self.mp_per_idx.get(last - smpl + n % smpl))
            .copied()
            .unwrap_or(f64::NAN);
        if self.b_additive {
            level + idx
        } else {
            level * idx
        }
    }

    fn get_forecast(&mut self, f_target: f64) -> f64 {
        self.init_calc();

        let last = self.mn_count - 1;
        let (f_from, in_range) = if f_target <= self.ma_range[last].x {
            (self.ma_range[0].x, true)
        } else {
            (self.ma_range[last].x, false)
        };
        let n_steps = (f_target - f_from) / self.mf_step_size;
        if n_steps.is_nan() || n_steps >= MAX_FORECAST_STEPS {
            return f64::NAN;
        }
        let n = n_steps as usize;
        let f_interpolate = (f_target - f_from) % self.mf_step_size;
        let (mut r_forecast, f_fc_1) = if in_range {
            let n = n.min(last);
            (
                self.ma_range[n].y,
                self.mp_forecast.get(n + 1).copied().unwrap_or(f64::NAN),
            )
        } else {
            (
                self.forecast_after(n),
                self.forecast_after(n.saturating_add(1)),
            )
        };
        if f_interpolate >= CF_MIN_ABC_RESOLUTION {
            let f_interpolate_factor = f_interpolate / self.mf_step_size;
            r_forecast += f_interpolate_factor * (f_fc_1 - r_forecast);
        }
        r_forecast
    }

    pub(crate) fn get_forecast_range(&mut self, r_t_mat: &[f64]) -> Vec<f64> {
        r_t_mat
            .iter()
            .map(|&x| {
                let f_target = if self.mn_month_day != 0 {
                    self.convert_x_to_months(x)
                } else {
                    x
                };
                self.get_forecast(f_target)
            })
            .collect()
    }

    pub(crate) fn get_statistic_value(&mut self, r_type_mat: &[f64]) -> Vec<f64> {
        self.init_calc();

        r_type_mat
            .iter()
            .map(|&t| match t as i32 {
                1 => self.mf_alpha,
                2 => self.mf_gamma,
                3 => self.mf_beta,
                4 => self.mf_mase,
                5 => self.mf_smape,
                6 => self.mf_mae,
                7 => self.mf_rmse,
                8 => self.mf_step_size,
                9 => self.mn_smpl_in_prd as f64,
                _ => f64::NAN,
            })
            .collect()
    }

    pub(crate) fn get_samples_in_period(&self) -> f64 {
        self.mn_smpl_in_prd as f64
    }

    fn rand_dev(&mut self) -> f64 {
        // return a random deviation given the standard deviation
        let u = 0.5 + 0.5 * (self.mr_context.rand)();
        self.mf_rmse * (self.mr_context.gaussinv)(u)
    }

    /// The target as the distance from the end of the series, in the unit of the X-values.
    fn target_after_end(&self, x: f64) -> f64 {
        let f_target = if self.mn_month_day != 0 {
            self.convert_x_to_months(x)
        } else {
            x
        };
        f_target - self.ma_range[self.mn_count - 1].x
    }

    /// The number of steps from the end of the series to the farthest target; 0 when there is none.
    fn steps_to_farthest(&self, r_t_mat: &[f64]) -> usize {
        // find maximum target value and calculate size of scenario-arrays
        let f_max_target = r_t_mat.iter().copied().fold(r_t_mat[0], f64::max);
        let f_max_target = self.target_after_end(f_max_target);
        let n_size = (f_max_target / self.mf_step_size) as usize;
        if f_max_target % self.mf_step_size != 0.0 {
            return n_size.saturating_add(1);
        }
        n_size
    }

    /// The step before a target (the index into the per step intervals) and the part of a step past it;
    /// `None` for a target on the end of the series. CHANGE: the part is a fraction of the step (the
    /// original used the remainder in X units).
    fn interval_position(&self, x: f64) -> Option<(usize, f64)> {
        let f_target = self.target_after_end(x);
        let f_steps = f_target / self.mf_step_size - 1.0;
        if f_steps.is_nan() || f_steps <= -1.0 || f_steps >= MAX_FORECAST_STEPS {
            return None;
        }
        let f_factor = (f_target % self.mf_step_size) / self.mf_step_size;
        Some((f_steps.max(0.0) as usize, f_factor))
    }

    /// The simulated intervals of a series with seasonality, one per target.
    pub(crate) fn get_ets_prediction_intervals(
        &mut self,
        r_t_mat: &[f64],
        f_pi_level: f64,
    ) -> Vec<f64> {
        self.init_calc();
        if self.mn_error_value != FormulaError::NoError {
            return Vec::new();
        }
        let n_size = self.steps_to_farthest(r_t_mat);
        if n_size == 0 {
            self.mn_error_value = FormulaError::IllegalArgument;
            return Vec::new();
        }
        // CHANGE: limits of this port
        let scenario_bytes = n_size.saturating_mul(CN_SCENARIOS * std::mem::size_of::<f64>());
        if n_size > MAX_PI_STEPS
            || !(self.mr_context.budget)(Budget::Bytes(scenario_bytes))
            || !(self.mr_context.budget)(Budget::Steps(n_size * CN_SCENARIOS / STEP_VALUES + 1))
        {
            self.mn_error_value = FormulaError::Aborted;
            return Vec::new();
        }

        let (n_count, smpl, additive) = (self.mn_count, self.mn_smpl_in_prd, self.b_additive);
        let (base, trend) = (self.mp_base[n_count - 1], self.mp_trend[n_count - 1]);
        let (alpha, beta, gamma) = (self.mf_alpha, self.mf_beta, self.mf_gamma);
        let (mut x_scen_base, mut x_scen_trend) = (vec![0.0; n_size], vec![0.0; n_size]);
        let mut x_scen_per_idx = vec![0.0; n_size];
        let mut a_predictions = vec![Vec::with_capacity(CN_SCENARIOS); n_size];
        // CHANGE: the original has the additive and the multiplicative model as two copies of the loop
        let season = |level: f64, idx: f64| if additive { level + idx } else { level * idx };
        let deseason = |y: f64, idx: f64| if additive { y - idx } else { y / idx };

        // fill scenarios
        for _ in 0..CN_SCENARIOS {
            // fill array with forecasts, with rand_dev() added to the scenario value
            for i in 0..n_size {
                let (prev_base, prev_trend) = match i {
                    0 => (base, trend),
                    _ => (x_scen_base[i - 1], x_scen_trend[i - 1]),
                };
                let f_per_idx = if i < smpl {
                    self.mp_per_idx[n_count + i - smpl]
                } else {
                    x_scen_per_idx[i - smpl]
                };
                let x_scen_range = season(prev_base + prev_trend, f_per_idx) + self.rand_dev();
                a_predictions[i].push(x_scen_range);
                x_scen_base[i] = alpha * deseason(x_scen_range, f_per_idx)
                    + (1.0 - alpha) * (prev_base + prev_trend);
                x_scen_trend[i] = gamma * (x_scen_base[i] - prev_base) + (1.0 - gamma) * prev_trend;
                x_scen_per_idx[i] =
                    beta * deseason(x_scen_range, x_scen_base[i]) + (1.0 - beta) * f_per_idx;
            }
        }

        // create array of Percentile values;
        let percentile = self.mr_context.percentile;
        let x_percentile: Vec<f64> = a_predictions
            .iter_mut()
            .map(|p| percentile(p, (1.0 + f_pi_level) / 2.0) - percentile(p, 0.5))
            .collect();
        self.intervals_at(r_t_mat, |n_steps| {
            x_percentile.get(n_steps).copied().unwrap_or(f64::NAN)
        })
    }

    /// The interval of each target from the interval `at(steps)` of whole steps, interpolated.
    fn intervals_at(&mut self, r_t_mat: &[f64], at: impl Fn(usize) -> f64) -> Vec<f64> {
        let mut intervals = Vec::with_capacity(r_t_mat.len());
        for &x in r_t_mat {
            let Some((n_steps, f_factor)) = self.interval_position(x) else {
                self.mn_error_value = FormulaError::IllegalFPOperation;
                return Vec::new();
            };
            let f_pi = at(n_steps);
            intervals.push(if f_factor != 0.0 {
                // interpolate
                f_pi + f_factor * (at(n_steps + 1) - f_pi)
            } else {
                f_pi
            });
        }
        intervals
    }

    /// The closed form intervals of a series without seasonality.
    pub(crate) fn get_eds_prediction_intervals(
        &mut self,
        r_t_mat: &[f64],
        f_pi_level: f64,
    ) -> Vec<f64> {
        self.init_calc();
        if self.mn_error_value != FormulaError::NoError {
            return Vec::new();
        }
        if self.steps_to_farthest(r_t_mat) == 0 {
            self.mn_error_value = FormulaError::IllegalArgument;
            return Vec::new();
        }

        let z = (self.mr_context.gaussinv)((1.0 + f_pi_level) / 2.0);
        let o = 1.0 - f_pi_level;
        // CHANGE: the coefficient of step i is computed where it is used; the original keeps a list
        // of one per step up to the farthest target, which is not bounded
        let c = |i: usize| -> f64 {
            let i = i as f64;
            (1.0 + (f_pi_level / (1.0 + o).powf(3.0))
                * ((1.0 + 4.0 * o + 5.0 * o * o)
                    + 2.0 * i * f_pi_level * (1.0 + 3.0 * o)
                    + 2.0 * (i * i) * f_pi_level * f_pi_level))
                .sqrt()
        };
        let rmse = self.mf_rmse;
        self.intervals_at(r_t_mat, |n_steps| z * rmse * c(n_steps) / c(0))
    }
}
