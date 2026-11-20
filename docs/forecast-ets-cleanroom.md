# FORECAST.ETS 계열 클린룸 명세

`FORECAST.ETS`, `FORECAST.ETS.CONFINT`, `FORECAST.ETS.SEASONALITY`, `FORECAST.ETS.STAT`의 수식과 선택 사항을 공개 자료만으로 정한 기록이다. 구현은 `crates/easyoffice-sheet/src/functions_forecast_ets_cr.rs`(Apache-2.0)다. 이 문서는 구현보다 먼저 썼고, 구현의 모든 판단이 여기에 근거를 둔다.

## 0. 클린룸 조건

- 함수 문서(S1~S5), 공개 교과서(S6~S12), 논문(S13~S14)만 읽었다.
- 이 함수를 다른 소스 트리에서 옮긴 코드, 그 코드를 인용·요약한 글은 보지 않았다.
- 기준 앱의 실제 출력값은 아직 없다. 값이 필요한 판단은 5장 "선택한 방법"에 모았고, 실측이 오면 `crates/easyoffice-sheet/src/functions_forecast_ets_cr_tests.rs`의 `#[ignore]` 시험이 이를 대조한다.
- 출처 표기: `[S번호 장·절]`. 온라인 교과서는 쪽 번호가 없어 절 번호와 식 번호로 적는다. 종이책을 인용한 곳은 절만 적고 쪽은 확인하지 못했다고 밝힌다.

## 1. 인수와 오류 (함수 문서)

| 함수 | 인수 |
| --- | --- |
| `FORECAST.ETS` | `target_date, values, timeline, [seasonality], [data_completion], [aggregation]` [S1] |
| `FORECAST.ETS.CONFINT` | `target_date, values, timeline, [confidence_level], [seasonality], [data_completion], [aggregation]` [S2] |
| `FORECAST.ETS.SEASONALITY` | `values, timeline, [data_completion], [aggregation]` [S3] |
| `FORECAST.ETS.STAT` | `values, timeline, statistic_type, [seasonality], [data_completion], [aggregation]` [S4] |

문서가 정한 값과 오류 [S1~S4 "Syntax" 절의 인수 설명]:

- `target_date`: 날짜 또는 숫자. 기록 시간축의 끝보다 앞이면 `#NUM!`.
- `timeline`: 숫자 배열. 간격이 일정하고 0이 아니어야 한다. 정렬되어 있지 않아도 된다(함수가 안에서 정렬). 일정한 간격을 찾을 수 없으면 `#NUM!`. 중복 값이 있으면 `#VALUE!`. `values`와 크기가 다르면 `#N/A`.
- `seasonality`: 기본 1은 자동 탐지, 0은 계절성 없음("예측이 선형"), 양의 정수는 그 길이의 주기. 그 밖의 값은 `#NUM!`. 최대 8,760(1년의 시간 수), 넘으면 `#NUM!`.
- `data_completion`: 시간축에서 누락된 점이 30%까지 지원된다. 0은 누락을 0으로, 기본 1은 이웃한 점의 평균으로 채운다.
- `aggregation`: 같은 시각의 값을 합치는 방법. 기본 0은 AVERAGE, 나머지는 SUM, COUNT, COUNTA, MIN, MAX, MEDIAN이다. 문서는 숫자와 방법의 대응표를 싣지 않는다(5.8절).
- `confidence_level`: 0과 1 사이(양 끝 제외), 기본 95%. 범위 밖은 `#NUM!`. 문서는 "90% 신뢰 수준이면 미래 점의 90%가 예측에서 이 반경 안에 든다"고 설명한다. 함수는 이 반경(반폭)을 돌려준다.
- `statistic_type`: 1~8의 숫자. 문서는 번호표 없이 8개 통계를 이 순서로 나열한다: α, β, γ, MASE, SMAPE, MAE, RMSE, 감지된 간격. MAE 항목의 설명 문장은 SMAPE와 같다(문서 오기로 보고 MAE로 읽는다) [S4 "The following optional statistics can be returned"].
- 모형: `FORECAST.ETS`는 지수 평활의 "AAA 버전"이다 [S1 첫 문단]. 덧셈 오차, 덧셈 추세, 덧셈 계절 성분이다(ETS(A,A,A), 분류는 S8 8.5절).

## 2. 모형 수식

### 2.1 상태 공간 형태

관측 y_1..y_T, 계절 주기 m(2 이상). 아래 식에서 ε_t는 한 걸음 앞 예측 오차다. [S8 8.5절 ETS(A,A,A)]

```
y_t = l_{t-1} + b_{t-1} + s_{t-m} + ε_t
l_t = l_{t-1} + b_{t-1} + α ε_t
b_t = b_{t-1} + β ε_t
s_t = s_{t-m} + γ ε_t
```

계절성이 없으면(m이 0 또는 1) s 항을 뺀다. 이것이 ETS(A,A,N)(Holt 선형 추세)다 [S8 8.5절 ETS(A,A,N)].

### 2.2 평활 계수의 표기

상태 공간의 β와 γ는 Holt-Winters 성분 형태의 β*, γ*와 이렇게 이어진다: `β = α β*`, `γ = γ* (1 - α)` [S6 8.3절, S8 8.5절]. 성분 형태의 식은 [S6 8.3절]:

```
ŷ_{t+h|t} = l_t + h b_t + s_{t+h-m(k+1)}      k = floor((h-1)/m)
l_t = α (y_t - s_{t-m}) + (1-α)(l_{t-1} + b_{t-1})
b_t = β* (l_t - l_{t-1}) + (1-β*) b_{t-1}
s_t = γ (y_t - l_{t-1} - b_{t-1}) + (1-γ) s_{t-m}
```

두 형태는 같은 갱신을 낸다(ε_t = y_t - l_{t-1} - b_{t-1} - s_{t-m}을 넣으면 위 4식이 그대로 나온다). 구현은 오차 보정 형태로 돌리고, 사용자에게는 α, β*, γ*를 돌려준다(5.4절).

### 2.3 예측

h걸음 앞(h는 1 이상의 정수): `ŷ_{T+h} = l_T + h b_T + s_{T+h-m(k+1)}` [S6 8.3절 예측식]. 마지막 한 주기의 계절 값만 쓴다.

## 3. 초기값과 평활 계수 최적화

### 3.1 목적함수

덧셈 오차에서는 우도 최대화와 제곱오차합(SSE) 최소화가 같은 해를 준다 [S9 8.6절 "Estimating ETS models"]. SSE = Σ (y_t - ŷ_{t|t-1})² [S10 8.1절 식 8.2].

### 3.2 초기 상태 (선택한 방법, 5.2절)

초기 상태(l_0, b_0, s_{1-m..0})는 교과서에서 평활 계수와 함께 최적화하는 것이 원칙이다 [S9 8.6절]. 주기가 8,760까지 올라가 상태가 8,762개가 될 수 있어 같이 최적화하지 않고, 데이터에서 직접 계산하는 값으로 고정한다(5.2절).

### 3.3 평활 계수 (선택한 방법, 5.3절)

α, β*, γ*를 [0, 1]에서 찾는다. 전통적 제약 0<α, β*, γ*<1이 상태 공간 제약 0<β<α, 0<γ<1-α와 같다 [S9 8.6절 "Parameter constraints"]. 탐색은 격자 → 좌표 탐색(패턴 탐색) 순서이고 도함수를 쓰지 않는다 [S14 직접 탐색 개요]. 평가 횟수에 상한을 둔다(6장).

## 4. 계절성 자동 탐지

자기상관의 정의 `r_k = Σ_{t=k+1}^{T} (y_t-ȳ)(y_{t-k}-ȳ) / Σ (y_t-ȳ)²`, 계절 데이터는 계절 간격의 시차에서 자기상관이 크다, 백색잡음의 95% 경계는 ±1.96/√T다 [S11 2.8절, 2.9절]. 한 번 차분(`y'_t = y_t - y_{t-1}`)은 추세와 계절의 수준 변화를 줄인다 [S12 9.1절 "Differencing"].

이 근거로 아래 방법을 골랐다(선택한 방법, 5.5절).

1. 격자로 완성한 계열(아래 7장 순서)에 한 번 차분을 하고 평균을 뺀다. 길이 N-1.
2. 시차 k = 1..K의 자기상관 r_k를 구한다. K = min(8,760, ⌊N/2⌋).
3. 후보 시차는 `2 ≤ k ≤ K`, `r_k > 1.96/√(N-1)`, `r_k ≥ r_{k-1}`, `r_k > r_{k+1}`(r_{K+1}은 -∞)인 k다.
4. 후보가 없으면 주기 1(계절성 없음). 있으면 가장 큰 r_k의 0.9배 이상인 후보 가운데 가장 작은 k가 주기다. 주기의 배수가 거의 같은 값을 갖기 때문에 가장 짧은 주기를 고른다.
5. 차분한 계열의 분산이 0이면(상수·선형 계열) 주기 1이다.

`FORECAST.ETS.SEASONALITY`는 이 주기를 돌려준다. 계절성이 없으면 1이다(5.5절).

## 5. 선택한 방법 (문서에 근거가 없는 부분)

함수 문서는 알고리즘 세부를 적지 않는다("AAA 버전", "30% 누락 지원" 같은 요약뿐). 아래는 이 구현이 직접 정한 것이다. 기준 앱 실측이 오면 이 장을 먼저 고친다.

1. **모형 계열**: 계절이 있으면 ETS(A,A,A), 없으면 ETS(A,A,N). 문서의 "계절성 0은 선형 예측"을 Holt 선형 추세(적합 가능한 α, β*)로 읽었다. 직선 회귀로 읽으면 STAT의 α, β를 정의할 수 없다.
2. **초기 상태**: 교과서의 휴리스틱 초기화(S13 2.6.1절)가 있다는 사실만 알고 세부는 보지 못했다. 직접 만든 방법이다.
   - 계절 없음: 앞 min(10, N)점의 최소제곱 직선. l_0는 시각 0의 절편, b_0는 기울기.
   - 계절 있음(N ≥ 2m): 앞 K = min(⌊N/m⌋, 4)개 계절의 블록 평균 μ_j를 블록 중심 시각 c_j = jm + (m+1)/2에 직선으로 맞춰 l_0(시각 0의 값), b_0(기울기)를 얻는다. s_i는 `y_{i+jm} - (l_0 + b_0 (i+jm))`의 j 평균이다. 마지막에 s의 평균을 빼고 그만큼 l_0에 더한다(합 0 정규화).
3. **평활 계수 최적화**: 모든 계수는 [0,1] 상자에서 찾는다. 1단계는 각 계수에 6개 값(0.02, 0.1, 0.25, 0.5, 0.75, 0.95)을 쓰는 격자다. 2단계는 격자 상위 3점에서 출발하는 좌표 탐색(초기 보폭 0.1, 개선이 없으면 절반, 보폭 1e-4 미만이면 종료). 평가 횟수 상한은 3,000이다. 값이 같으면(예: 완전한 직선에서 SSE 0) 먼저 찾은 점을 쓴다. 이 방법은 교과서의 전역 최적화와 정확히 같은 해를 보장하지 않는다.
4. **STAT의 α, β, γ**: Holt-Winters 성분 형태의 α, β*, γ*를 돌려준다(2.2절). 문서가 "β가 클수록 최근 추세에 가중, γ가 클수록 최근 계절 주기에 가중"이라고 설명해 β*, γ*의 뜻과 맞는다. 계절성이 없으면 γ는 0이다.
5. **계절성 탐지 세부**: 4장의 차분 방식, 임계값, 0.9 규칙, 탐지 실패 시 주기 1은 모두 직접 정했다. 근거는 S11, S12의 일반 설명뿐이다.
6. **오차 분산 σ²**: `σ² = SSE / T`(T는 완성한 점 수). 교과서가 "잔차 분산"이라고만 적어(S7 8.7절) 최대우도 추정량을 골랐다.
7. **신뢰구간**: `FORECAST.ETS.CONFINT = z σ_h`, `z = Φ⁻¹((1+c)/2)`. σ_h는 S7 8.7절 표 8.7의 ETS(A,A,A) 식이다. m이 0 또는 1이면 γ=0, 계절 항(k 항)이 없는 ETS(A,A,N) 식이다. 상태 공간의 β = α β*, γ = γ*(1-α)를 대입한다.
   ```
   σ_h² = σ² [ 1 + (h-1){α² + αβh + β²h(2h-1)/6} + γ k {2α + γ + β m (k+1)} ],  k = floor((h-1)/m)
   ```
   `h ≤ 1`이면 σ_h = σ. h가 정수가 아니면 5.9절의 보간을 쓴다.
8. **집계 번호**: 문서는 이름만 나열하고 번호를 적지 않는다. 번호를 정한 근거는 이름의 알파벳 순서뿐이다.

   | 번호 | 방법 |
   | --- | --- |
   | 0, 1 | AVERAGE |
   | 2 | COUNT |
   | 3 | COUNTA |
   | 4 | MAX |
   | 5 | MEDIAN |
   | 6 | MIN |
   | 7 | SUM |

   0과 1을 둘 다 AVERAGE로 둔 것은 문서의 "기본 0은 AVERAGE"와 작업 지시의 "1~7"을 같이 만족시키는 선택이다. 숫자만 받는 수치 API에서 COUNT와 COUNTA는 같은 값(점 개수)이다. `aggregation`을 명시하면 한 번만 나온 시각도 같은 방법을 지난다(COUNT는 1). 8 이상, 음수, 정수가 아닌 값은 `#NUM!`.
   **중복 시각의 처리**: 문서는 한쪽에서 "중복 값이 있으면 `#VALUE!`", 다른 쪽에서 "같은 시각의 값을 집계한다"고 적는다. 둘을 이렇게 맞췄다: `aggregation` 인수를 생략하면 중복 시각은 `#VALUE!`, 명시하면 집계한다.
9. **비정수 h(시간축 격자에 맞지 않는 `target_date`)**: `h = (target - t_last) / step`이 정수가 아니면 ⌊h⌋와 ⌈h⌉의 예측(과 σ_h)을 선형 보간한다. h = 0(마지막 시각)은 마지막 관측의 한 걸음 앞 적합값이다. 목표가 끝보다 앞서면 `#NUM!`(문서).
10. **간격 찾기**: 정렬한 시간축의 가장 작은 양의 차이를 간격으로 두고, 모든 차이가 그 간격의 정수배(오차 1e-6)인지 본다. 아니면 `#NUM!`. 달 단위 날짜처럼 일수가 달라지는 시간축은 호출하는 쪽이 달 번호로 바꿔 넘긴다.
11. **누락 처리**: 간격의 정수배에 빈 칸이 있으면 누락이다. 누락 비율이 30%를 넘으면 `#NUM!`(문서는 30%까지 지원이라고만 적는다). `data_completion = 0`은 0으로, `1`은 이웃한 점 사이의 선형 보간(점 하나가 빠진 경우 이웃 평균)으로 채운다. 그 밖의 값은 `#NUM!`.
12. **최소 점 수**: 집계 뒤 3점 미만이면 `#NUM!`. 명시한 주기 m에서 N < 2m이면 `#NUM!`.
13. **MASE, SMAPE, MAE, RMSE**: 모두 학습 구간의 한 걸음 앞 오차 e_t = y_t - ŷ_{t|t-1}(t=1..N)로 계산한다.
    - MAE = mean|e_t|, RMSE = sqrt(mean e_t²) [S15 5.8절].
    - SMAPE = mean(200 |y_t - ŷ_t| / (|y_t| + |ŷ_t|)) [S15 5.8절 식에서 분모를 절댓값으로 바꿨다. 양수 데이터에서는 같다. 퍼센트 단위]. 분모가 0인 항은 0으로 센다. 기준 앱이 비율(0~2)로 돌려줄 가능성이 있다.
    - MASE = mean|e_t| / (Σ_{t=m'+1}^{N}|y_t - y_{t-m'}| / (N-m')), 계절이 있으면 m' = m, 없으면 m' = 1 [S15 5.8절 "Scaled errors"]. 분모가 0이면 `#NUM!`.
    - 감지된 간격(8번)은 정렬한 시간축의 간격이다.
14. **`FORECAST.ETS.SEASONALITY` 반환**: 탐지 결과의 주기, 없으면 1(5.5절).

## 6. 상한과 Fail-Closed

신뢰할 수 없는 입력(열자마자 계산되는 문서의 수식)에 대비해 아래를 넘으면 오류를 돌려주고 계산하지 않는다. 판단은 모두 메모리를 잡기 전에 한다.

| 항목 | 상한 | 넘으면 |
| --- | --- | --- |
| 입력 점 수 | 50,000 | `#NUM!` |
| 완성한 격자 길이 | 50,000 | `#NUM!` |
| 계절 주기 | 8,760 [S1] | `#NUM!` |
| 최적화 평가 횟수 | 3,000 | 중단하고 그때까지의 최선을 쓴다 |
| 목표까지의 걸음 수 h | 1e9 | `#NUM!` |
| 비유한 값(NaN, ±∞)이 입력에 있음 | 허용 안 함 | `#VALUE!` |
| 계산 결과가 비유한 | 허용 안 함 | `#NUM!` |

자기상관 계산은 N×K 곱셈(최대 2.2e8)이고, 적합 한 번은 O(N)이다. 릴리스 빌드에서 50,000점은 0.07~0.55초였다(주기 8,760 명시 0.07초, 탐지 0.47초, 선형 0.54초).

## 7. 처리 순서

1. 크기가 다르면 `#N/A`. 빈 입력은 `#NUM!`.
2. 인수 검증(seasonality, data_completion, aggregation, confidence_level, statistic_type). 위반은 `#NUM!`.
3. 비유한 값은 `#VALUE!`.
4. 시간축을 정렬한다. 중복 시각을 집계하거나(8번) `#VALUE!`.
5. 간격을 찾고, 누락을 세고, 상한을 확인한다.
6. 격자를 만들고 누락을 채운다.
7. 주기를 정한다(탐지 또는 명시).
8. 초기값, 평활 계수 최적화, 최종 상태.
9. 예측, 신뢰구간 또는 통계.

## 8. 출처 목록

| 번호 | 자료 | 위치 |
| --- | --- | --- |
| S1 | 기준 앱 공식 함수 문서, `FORECAST.ETS` 항목 (영문 판, 한국어 판 대조) | URL은 `THIRD_PARTY_NOTICES.md`의 "FORECAST.ETS clean-room reference material" |
| S2 | 같은 문서, `FORECAST.ETS.CONFINT` 항목 | 같은 곳 |
| S3 | 같은 문서, `FORECAST.ETS.SEASONALITY` 항목 | 같은 곳 |
| S4 | 같은 문서, `FORECAST.ETS.STAT` 항목 | 같은 곳 |
| S5 | 같은 문서, Forecasting functions 목록 | 같은 곳 |
| S6 | Hyndman & Athanasopoulos, *Forecasting: Principles and Practice*, 3판, 8.3 Methods with seasonality (Holt-Winters 가법) | https://otexts.com/fpp3/holt-winters.html |
| S7 | 같은 책, 8.7 Forecasting with ETS models (예측 구간, 표 8.7) | https://otexts.com/fpp3/ets-forecasting.html |
| S8 | 같은 책, 8.5 Innovations state space models for exponential smoothing | https://otexts.com/fpp3/ets.html |
| S9 | 같은 책, 8.6 Estimation and model selection (제약, 우도) | https://otexts.com/fpp3/ets-estimation.html |
| S10 | 같은 책, 8.1 Simple exponential smoothing, "Optimisation" (식 8.2) | https://otexts.com/fpp3/ses.html |
| S11 | 같은 책, 2.8 Autocorrelation, 2.9 White noise | https://otexts.com/fpp3/acf.html |
| S12 | 같은 책, 9.1 Stationarity and differencing | https://otexts.com/fpp3/stationarity.html |
| S13 | Hyndman, Koehler, Ord & Snyder, *Forecasting with Exponential Smoothing: The State Space Approach*, Springer 2008, ISBN 978-3-540-71916-8. 2장(초기화), 6장(예측 분산)은 S7이 가리키는 것만 알고 본문은 보지 못했다. 쪽 번호 미확인 | 종이책 |
| S14 | Kolda, Lewis & Torczon, "Optimization by Direct Search: New Perspectives on Some Classical and Modern Methods", *SIAM Review* 45(3), 385-482, 2003 | doi:10.1137/S0036144502428893 |
| S15 | Hyndman & Athanasopoulos, 같은 책 5.8 Evaluating point forecast accuracy (MAE, RMSE, sMAPE, MASE) | https://otexts.com/fpp3/accuracy.html |

제품 이름이 든 자료의 이름과 URL은 저장소 지침에 따라 `THIRD_PARTY_NOTICES.md`에만 둔다. 시험 데이터는 Box, Jenkins & Reinsel의 월별 항공 승객 수 144개(1949-1960, 공개 데이터셋)다. 대조용 수치는 BSD 라이선스의 파이썬 통계 라이브러리 `ETSModel.smooth`에 고정한 초기 상태와 평활 계수를 넣어 얻었다(코드는 가져오지 않았고 결과 숫자만 시험에 적었다).

## 9. 남은 일 (기준 앱 실측 필요)

- 5.1(선형 모형의 정의), 5.5(탐지 규칙과 "없음"의 반환값), 5.8(집계 번호, 중복 시각), 5.11(누락 30% 초과), 5.13(SMAPE 단위, MAE·RMSE의 구간)은 실측 없이는 맞는지 모른다.
- 값이 오면 `functions_forecast_ets_cr_tests.rs`의 `#[ignore]` 시험에 채우고 이 장을 지운다.
