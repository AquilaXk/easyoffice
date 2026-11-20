# 시트 함수 비교와 이식 현황 (PD-270)

기준 앱의 함수 사전순 문서 목록과 우리 엔진, 참조 엔진 셋을 이름 기준으로 맞춘 표다. 참조 엔진 A는 Rust, B와 C는 TypeScript와 JavaScript 구현이며 이름과 출처는 `THIRD_PARTY_NOTICES.md`에 있다. 목록은 2026-11-20 기준이고 "있음"은 이름을 부르면 `#NAME?`이 아닌 값이 나오는 것을 뜻한다(등록 여부가 아니라 실제 호출로 확인).

## 요약

- 기준 목록 521개 중 우리 엔진은 **494개**(이번 작업 전 480개), 없는 것은 **27개**(전 41개)다. 기준 목록에 없는 우리 함수는 6개다(`AVERAGE.WEIGHTED`, `IMCOTH`, `IMLOG`, `IMTANH`, `MARGINOFERROR`, `ROUNDBANK`).
- 참조 엔진: A 496개, B 497개, C 403개가 기준 목록에 든다.
- 없는 27개 가운데 참조 엔진 어느 쪽에도 없는 것은 17개다.

## 범주별 수

| 범주 | 기준 | 우리 | 없음 | 참조 A | 참조 B | 참조 C |
|---|---:|---:|---:|---:|---:|---:|
| 통계 | 111 | 107 | 4 | 111 | 107 | 104 |
| 수학·삼각 | 80 | 80 | 0 | 79 | 80 | 74 |
| 재무 | 55 | 55 | 0 | 55 | 55 | 33 |
| 공학 | 54 | 54 | 0 | 54 | 54 | 52 |
| 텍스트 | 50 | 45 | 5 | 44 | 46 | 28 |
| 호환성 | 40 | 40 | 0 | 40 | 40 | 30 |
| 조회·참조 | 40 | 35 | 5 | 35 | 37 | 20 |
| 날짜·시간 | 25 | 25 | 0 | 25 | 25 | 25 |
| 정보 | 22 | 21 | 1 | 21 | 19 | 14 |
| 논리 | 19 | 19 | 0 | 19 | 19 | 11 |
| 데이터베이스 | 12 | 12 | 0 | 12 | 12 | 12 |
| 큐브 | 7 | 0 | 7 | 0 | 0 | 0 |
| 추가 기능·자동화 | 3 | 0 | 3 | 0 | 1 | 0 |
| 웹 | 3 | 1 | 2 | 1 | 2 | 0 |

## 없는 함수 27개

| 범주 | 함수 | 참조 | 사정 |
|---|---|---|---|
| 통계 | `FORECAST.ETS` | A | 지수 평활 예측. 참조 A에만 있다. 분량이 커 TODO. |
| 통계 | `FORECAST.ETS.CONFINT` | A | 위와 같다. |
| 통계 | `FORECAST.ETS.SEASONALITY` | A | 위와 같다. |
| 통계 | `FORECAST.ETS.STAT` | A | 위와 같다. |
| 텍스트 | `BAHTTEXT` | B | 태국어 금액 읽기. 참조 B에만 있다. 사용처가 좁아 TODO. |
| 텍스트 | `DETECTLANGUAGE` | - | 온라인 번역 서비스. 대상 아님. |
| 텍스트 | `JIS` | - | 일본어 전각 변환. 참조 엔진에 없다. TODO. |
| 텍스트 | `PHONETIC` | - | 후리가나 정보가 필요하다. 참조 엔진에 없다. TODO. |
| 텍스트 | `TRANSLATE` | - | 위와 같다. |
| 조회·참조 | `GETPIVOTDATA` | B | 피벗 표 조회. 피벗 엔진과 맞춰야 한다. |
| 조회·참조 | `GROUPBY` | B | 피벗 집계 함수. 참조 B에 이름만 있다. 피벗 엔진과 맞춰야 한다. |
| 조회·참조 | `IMAGE` | B | 셀 안 그림. 시트 개체 모델이 필요하다. |
| 조회·참조 | `PIVOTBY` | - | 위와 같다. 참조 엔진에 없다. |
| 조회·참조 | `RTD` | - | 실시간 데이터 서버. 외부 연결이라 대상 아님. |
| 정보 | `STOCKHISTORY` | - | 온라인 시세. 대상 아님. |
| 큐브 | `CUBEKPIMEMBER` | - | 큐브(OLAP) 연결이 필요하다. 대상 아님. |
| 큐브 | `CUBEMEMBER` | - | 큐브(OLAP) 연결이 필요하다. 대상 아님. |
| 큐브 | `CUBEMEMBERPROPERTY` | - | 큐브(OLAP) 연결이 필요하다. 대상 아님. |
| 큐브 | `CUBERANKEDMEMBER` | - | 큐브(OLAP) 연결이 필요하다. 대상 아님. |
| 큐브 | `CUBESET` | - | 큐브(OLAP) 연결이 필요하다. 대상 아님. |
| 큐브 | `CUBESETCOUNT` | - | 큐브(OLAP) 연결이 필요하다. 대상 아님. |
| 큐브 | `CUBEVALUE` | - | 큐브(OLAP) 연결이 필요하다. 대상 아님. |
| 추가 기능·자동화 | `CALL` | - | 외부 라이브러리 호출. 대상 아님. |
| 추가 기능·자동화 | `EUROCONVERT` | B | 유로 환산표가 필요하다. 참조 B에 있다. TODO. |
| 추가 기능·자동화 | `REGISTER.ID` | - | 위와 같다. |
| 웹 | `FILTERXML` | B | WEBSERVICE와 함께 쓰는 함수. 네트워크 없이도 XML 질의 엔진이 필요하다. |
| 웹 | `WEBSERVICE` | - | 네트워크 호출. 신뢰할 수 없는 문서에서 열면 안 되므로 대상 아님. |

## 이번에 이식한 함수 14개

원본 선택 순서(참조 A, B, C)대로 A에서 가져왔다. 함수 이름, 보조 함수 이름, 검사 순서, 분기, 오류값을 원본과 같게 두고 값 형식과 배열 평가는 얇은 어댑터로 흡수했다. 파일 머리에 원본 함수와 줄 번호를 1:1로 적었다.

| 범주 | 함수 | 위치 | 다르게 한 점 |
|---|---|---|---|
| 텍스트 | `TEXTSPLIT`, `ARRAYTOTEXT` | `functions_text_split.rs` | 대소문자 접기를 `char` 단위로 해 바이트 위치 어긋남을 없앴다. 구분자 수 × 글자 수 400만 회, 결과 글자 32,767자 상한(Fail-Closed). |
| 텍스트 | `REGEXTEST`, `REGEXEXTRACT`, `REGEXREPLACE` | `functions_regex.rs` | 패턴 길이 4,096바이트, 프로그램 크기·DFA 캐시 1 MiB, 중첩 64 상한(넘으면 `#VALUE!`). 반환 형식 0은 첫 일치 전체(참조 A는 첫 그룹), 형식 2·대소문자 인수·`REGEXREPLACE` 횟수 인수는 문서에 맞춰 더했다. |
| 조회·참조 | `TRIMRANGE` | `functions_array_shape.rs` | 결과 크기를 배열 예산에 건다. 전체 열 참조는 아직 `#SPILL!`. |
| 정보 | `ISREF`, `SHEET`, `SHEETS`, `CELL`, `INFO` | `functions_info_ref.rs` | 참조 판정은 이 엔진이 참조로 푸는 식(`OFFSET`·`INDIRECT`·`INDEX`·`CHOOSE`·이름)이다. 파일 이름·디렉터리·릴리스가 필요한 종류는 `#N/A`. |
| 수학 | `PERCENTOF`, `ISO.CEILING` | `functions_math.rs`, `functions_math_trig.rs` | `ISO.CEILING`은 `CEILING.PRECISE`와 같은 길이다. |
| 재무 | `AMORDEGRC` | `functions_depr.rs` | 일수 계산은 이 엔진의 기준별 일수 함수를 쓴다. 문서 예제 776과 일치. |

옮긴 줄과 직접 쓴 줄은 커밋 보고에 있다. 정규식 함수는 엔진 크레이트 외부 의존 0 규칙(PD-053)이 QA 결정으로 폐지되어(PD-269) `regex` 크레이트로 옮겼다.

## 정확도 수정 30건

기준은 함수 문서의 예제와 규칙이다. 참조 A나 B가 문서와 다르면 문서를 따랐고 그 경우는 "참조와 다름"으로 적었다.

| 번호 | 함수 | 바꾼 것 | 근거 |
|---|---|---|---|
| 1 | `ADDRESS` | 상대 R1C1은 행·열 번호를 대괄호에 그대로 쓴다(`ADDRESS(2,3,2,FALSE)` = `R2C[3]`). 전에는 수식 칸에서 센 차이를 썼다. | 문서 예제. 참조 A·B와 다름 |
| 2 | `TYPE` | 배열(범위, 배열 상수, 배열을 돌려주는 함수)은 64. | 문서 규칙 |
| 3 | `VLOOKUP` | 열 번호 1 미만은 `#VALUE!`(전에는 `#REF!`). | 문서 규칙 |
| 4 | `HLOOKUP` | 행 번호 1 미만은 `#VALUE!`. | 문서 규칙 |
| 5 | `INDEX` | 행이나 열 번호 0 또는 생략은 그 열·행·전체를 참조로 돌려준다(`SUM(INDEX(A1:B2,0,2))`가 2열 합계). 전에는 첫 칸만 읽었다. | 문서 규칙 |
| 6 | `INDEX` | 2차원 범위에 번호 하나면 그 행 전체. | 문서 규칙 |
| 7 | `INDEX` | 배열 상수와 배열 결과에도 5, 6번을 적용한다. | 문서 규칙 |
| 8 | `INDEX` | 음수 번호는 `#VALUE!`. | 문서 규칙 |
| 9 | `INDEX` | 4번째 인수 `area_num`을 받는다(1 초과 `#REF!`, 1 미만 `#VALUE!`). | 문서 규칙 |
| 10 | `LOOKUP` | 결과 벡터가 찾는 벡터와 방향이 달라도 위치로 읽는다. 길이가 다르면 전과 같이 `#REF!`. | 참조 A와 같음 |
| 11 | `CHOOSE` | 고른 인수가 참조면 참조로 돌려준다(`SUM(CHOOSE(2,A1:A2,A1:A3))`). | 문서 규칙 |
| 12 | `INDIRECT` | 두 번째 인수 FALSE일 때 R1C1 글을 읽는다(절대·상대·범위). | 문서 규칙 |
| 13 | `WEEKDAY` | 반환 형식 11~17. | 문서 규칙 |
| 14 | `NETWORKDAYS`, `NETWORKDAYS.INTL`, `WORKDAY`, `WORKDAY.INTL` | 휴일 목록에 날짜가 아닌 글이 있으면 `#VALUE!`(전에는 무시). | 문서 규칙 |
| 15 | `NETWORKDAYS.INTL`, `WORKDAY.INTL` | 0과 1이 아닌 길이의 주말 문자열(`"111110"`)은 `#VALUE!`. | 문서 규칙 |
| 16 | `NETWORKDAYS.INTL`, `WORKDAY.INTL` | 빈 칸을 가리킨 주말 인수는 0으로 읽어 `#NUM!`. | 참조 A와 같음 |
| 17 | `ACCRINTM` | `par` 생략 가능(1000). 전에는 `#VALUE!`. | 문서 규칙 |
| 18 | `ACCRINTM` | 발행일이 결제일 이상이면 `#NUM!`(전에는 0). | 문서 규칙. 참조 B와 다름 |
| 19 | `BASE` | 음수는 0으로 잘리는 값(-0.5)도 `#NUM!`. | 문서 규칙 |
| 20 | `DECIMAL` | 256자 이상은 `#VALUE!`. | 문서 규칙 |
| 21 | `DECIMAL` | 빈 글은 0. | 참조 A와 같음 |
| 22 | `CHAR` | 255 초과는 `#VALUE!`(`UNICHAR`은 유니코드 전체). | 문서 규칙 |
| 23 | `CHAR` | 128~159는 Windows-1252 글자(128은 `€`). | 문서 규칙 |
| 24 | `CODE` | 위의 역변환(`CODE("€")` = 128). | 문서 규칙 |
| 25 | `IFERROR`, `IFNA` | 값 배열이 대체 배열보다 작으면 모자란 자리는 `#N/A`로 보고 대체 값을 쓴다. | 참조 A와 같음 |
| 26 | `ISREF` | 배열을 돌려주는 함수(`FILTER` 등)는 참조가 아니다. | 참조 A와 같음 |
| 27 | 텍스트 18개 | 배열 인수를 원소별로 계산: `UNICHAR`, `UNICODE`, `CLEAN`, `ENCODEURL`, `NUMBERVALUE`, `DOLLAR`, `FIXED`, `TEXTBEFORE`, `TEXTAFTER`, `LENB`·`LEFTB`·`RIGHTB`·`MIDB`·`FINDB`·`SEARCHB`·`REPLACEB`, `ASC`, `DBCS`. | 동적 배열 규칙 |
| 28 | 날짜 9개 | `WEEKDAY`, `WEEKNUM`, `ISOWEEKNUM`, `DATEVALUE`, `TIMEVALUE`, `EOMONTH`, `DAYS360`, `DATEDIF`, `YEARFRAC`. | 동적 배열 규칙 |
| 29 | 공학·통계 35개 | 비트 5개, 진법 변환 12개, `DELTA`, `GESTEP`, `ERF` 계열 4개, `PHI`, `GAUSS`, `FISHER`, `FISHERINV`, 정규분포 8개. | 동적 배열 규칙 |
| 30 | 수학 25개 | `ACOSH`, `ASINH`, `ATANH`, `ACOT`, `ACOTH`, `COT`, `COTH`, `CSC`, `CSCH`, `SEC`, `SECH`, `COMBIN`, `COMBINA`, `FACTDOUBLE`, `SQRTPI`, `ROUNDBANK`, `CEILING.MATH` 계열 4개, `BASE`, `DECIMAL`, `ARABIC`, `ROMAN`. | 동적 배열 규칙 |

추가 수정: 배열 원소가 된 빈 칸이 `Expr::Text("")`로 바뀌어 `SUM(ABS(A1:A3))`가 `#VALUE!`이던 것을 빈 칸 식(`Expr::Blank`)으로 넘기고, 숫자 인수의 빈 칸을 0으로 읽는다(`ABS(Z99)` = 0, `NOT` 빈 칸 = TRUE, `ISBLANK(범위)`).

27~30은 배열 인수가 들어갈 때만 새 길을 탄다. 한 칸 인수의 수식은 전과 같은 길이라 재계산 속도는 변하지 않는다.

## 참조 시험 이식

참조 A의 시험 파일 295개 가운데 이 엔진의 시험 모형으로 옮길 수 있는 127개(셀 쓰기·계산·읽기만 쓰는 것)를 실행해 772건 중 같은 결과가 나온 **528건**을 `crates/easyoffice-sheet/tests/sheet_ironcalc_cases/`에 옮겼다. 정규식 함수 시험은 19건 중 15건을 옮겼다(나머지는 참조 A의 내부 오류 코드와 첫 그룹 반환 차이).

같지 않은 243건은 이렇다.

- 99건: 참조 A의 내부 오류 코드(`#ERROR!`, 인수 개수 오류)를 기대한다. 이 엔진은 `#VALUE!`를 돌려주고 기준 앱은 식 입력을 거부하므로 해당 없음.
- 69건: 참조 A가 9자리로 줄여 보인 값. 수치는 같다.
- 75건: 실제 차이거나 입력 방식 차이다. 갈래는 아래와 같고 `TODO.md`에 한 줄씩 있다.
  - 괄호를 닫지 않은 식(`=BITAND(13, 25`)을 참조 A가 자동으로 닫는 입력(약 10건).
  - `@` 암묵적 교차 연산자와 `A1:OFFSET(...)` 같은 함수 끝 범위(약 8건).
  - 순환 참조가 `#CIRC!` 대신 이 엔진은 `#NUM!`(2건), `DATEDIF`·`DATEVALUE`의 지역 날짜 글(약 5건), `SEQUENCE`·`RANDARRAY` 한도 오류 값(`#SPILL!`/`#VALUE!`, 4건).
  - 빈 칸이 배열 원소로 함수에 넘어갈 때(`NOT` 등) 빈 글 취급(약 3건).
  - `ACCRINT` 기준 1~3, `ODDFPRICE` 일부(약 20건): 참조 A가 기준 앱 산출 값과 대조해 둔 시험이라 우리 쪽 일수 계산을 고쳐야 한다(`TODO.md`).
  - 그 밖(`ROUNDDOWN` 이진 오차 보정, `COLUMNS(@A1:C1)`, 배열 `SORT`의 검증 순서 등).


## 전체 비교표

"우리"는 이 브랜치 기준이다. O는 있음, -는 없음이다.

| 함수 | 우리 | 참조 A | 참조 B | 참조 C | 범주 |
|---|:-:|:-:|:-:|:-:|---|
| `AVEDEV` | O | O | O | O | 통계 |
| `AVERAGE` | O | O | O | O | 통계 |
| `AVERAGEA` | O | O | O | O | 통계 |
| `AVERAGEIF` | O | O | O | O | 통계 |
| `AVERAGEIFS` | O | O | O | O | 통계 |
| `BETA.DIST` | O | O | O | O | 통계 |
| `BETA.INV` | O | O | O | O | 통계 |
| `BINOM.DIST` | O | O | O | O | 통계 |
| `BINOM.DIST.RANGE` | O | O | O | O | 통계 |
| `BINOM.INV` | O | O | O | O | 통계 |
| `CHISQ.DIST` | O | O | O | O | 통계 |
| `CHISQ.DIST.RT` | O | O | O | O | 통계 |
| `CHISQ.INV` | O | O | O | O | 통계 |
| `CHISQ.INV.RT` | O | O | O | O | 통계 |
| `CHISQ.TEST` | O | O | O | O | 통계 |
| `CONFIDENCE.NORM` | O | O | O | O | 통계 |
| `CONFIDENCE.T` | O | O | O | O | 통계 |
| `CORREL` | O | O | O | O | 통계 |
| `COUNT` | O | O | O | O | 통계 |
| `COUNTA` | O | O | O | O | 통계 |
| `COUNTBLANK` | O | O | O | O | 통계 |
| `COUNTIF` | O | O | O | O | 통계 |
| `COUNTIFS` | O | O | O | O | 통계 |
| `COVARIANCE.P` | O | O | O | O | 통계 |
| `COVARIANCE.S` | O | O | O | O | 통계 |
| `DEVSQ` | O | O | O | O | 통계 |
| `EXPON.DIST` | O | O | O | O | 통계 |
| `F.DIST` | O | O | O | O | 통계 |
| `F.DIST.RT` | O | O | O | O | 통계 |
| `F.INV` | O | O | O | O | 통계 |
| `F.INV.RT` | O | O | O | O | 통계 |
| `F.TEST` | O | O | O | O | 통계 |
| `FISHER` | O | O | O | O | 통계 |
| `FISHERINV` | O | O | O | O | 통계 |
| `FORECAST` | O | O | O | O | 통계 |
| `FORECAST.ETS` | - | O | - | - | 통계 |
| `FORECAST.ETS.CONFINT` | - | O | - | - | 통계 |
| `FORECAST.ETS.SEASONALITY` | - | O | - | - | 통계 |
| `FORECAST.ETS.STAT` | - | O | - | - | 통계 |
| `FORECAST.LINEAR` | O | O | O | - | 통계 |
| `FREQUENCY` | O | O | O | O | 통계 |
| `GAMMA` | O | O | O | O | 통계 |
| `GAMMA.DIST` | O | O | O | O | 통계 |
| `GAMMA.INV` | O | O | O | O | 통계 |
| `GAMMALN` | O | O | O | O | 통계 |
| `GAMMALN.PRECISE` | O | O | O | O | 통계 |
| `GAUSS` | O | O | O | O | 통계 |
| `GEOMEAN` | O | O | O | O | 통계 |
| `GROWTH` | O | O | O | O | 통계 |
| `HARMEAN` | O | O | O | O | 통계 |
| `HYPGEOM.DIST` | O | O | O | O | 통계 |
| `INTERCEPT` | O | O | O | O | 통계 |
| `KURT` | O | O | O | O | 통계 |
| `LARGE` | O | O | O | O | 통계 |
| `LINEST` | O | O | O | O | 통계 |
| `LOGEST` | O | O | O | O | 통계 |
| `LOGNORM.DIST` | O | O | O | O | 통계 |
| `LOGNORM.INV` | O | O | O | O | 통계 |
| `MAX` | O | O | O | O | 통계 |
| `MAXA` | O | O | O | O | 통계 |
| `MAXIFS` | O | O | O | O | 통계 |
| `MEDIAN` | O | O | O | O | 통계 |
| `MIN` | O | O | O | O | 통계 |
| `MINA` | O | O | O | O | 통계 |
| `MINIFS` | O | O | O | O | 통계 |
| `MODE.MULT` | O | O | O | O | 통계 |
| `MODE.SNGL` | O | O | O | O | 통계 |
| `NEGBINOM.DIST` | O | O | O | O | 통계 |
| `NORM.DIST` | O | O | O | O | 통계 |
| `NORM.S.DIST` | O | O | O | O | 통계 |
| `NORM.S.INV` | O | O | O | O | 통계 |
| `NORMINV` | O | O | O | O | 통계 |
| `PEARSON` | O | O | O | O | 통계 |
| `PERCENTILE.EXC` | O | O | O | O | 통계 |
| `PERCENTILE.INC` | O | O | O | O | 통계 |
| `PERCENTRANK.EXC` | O | O | O | O | 통계 |
| `PERCENTRANK.INC` | O | O | O | O | 통계 |
| `PERMUT` | O | O | O | O | 통계 |
| `PERMUTATIONA` | O | O | O | O | 통계 |
| `PHI` | O | O | O | O | 통계 |
| `POISSON.DIST` | O | O | O | O | 통계 |
| `PROB` | O | O | O | O | 통계 |
| `QUARTILE.EXC` | O | O | O | O | 통계 |
| `QUARTILE.INC` | O | O | O | O | 통계 |
| `RANK.AVG` | O | O | O | O | 통계 |
| `RANK.EQ` | O | O | O | O | 통계 |
| `RSQ` | O | O | O | O | 통계 |
| `SKEW` | O | O | O | O | 통계 |
| `SKEW.P` | O | O | O | O | 통계 |
| `SLOPE` | O | O | O | O | 통계 |
| `SMALL` | O | O | O | O | 통계 |
| `STANDARDIZE` | O | O | O | O | 통계 |
| `STDEV.P` | O | O | O | O | 통계 |
| `STDEV.S` | O | O | O | O | 통계 |
| `STDEVA` | O | O | O | O | 통계 |
| `STDEVPA` | O | O | O | O | 통계 |
| `STEYX` | O | O | O | O | 통계 |
| `T.DIST` | O | O | O | O | 통계 |
| `T.DIST.2T` | O | O | O | - | 통계 |
| `T.DIST.RT` | O | O | O | O | 통계 |
| `T.INV` | O | O | O | O | 통계 |
| `T.INV.2T` | O | O | O | - | 통계 |
| `T.TEST` | O | O | O | O | 통계 |
| `TREND` | O | O | O | O | 통계 |
| `TRIMMEAN` | O | O | O | O | 통계 |
| `VAR.P` | O | O | O | O | 통계 |
| `VAR.S` | O | O | O | O | 통계 |
| `VARA` | O | O | O | O | 통계 |
| `VARPA` | O | O | O | O | 통계 |
| `WEIBULL.DIST` | O | O | O | O | 통계 |
| `Z.TEST` | O | O | O | O | 통계 |
| `ABS` | O | O | O | O | 수학·삼각 |
| `ACOS` | O | O | O | O | 수학·삼각 |
| `ACOSH` | O | O | O | O | 수학·삼각 |
| `ACOT` | O | O | O | O | 수학·삼각 |
| `ACOTH` | O | O | O | O | 수학·삼각 |
| `AGGREGATE` | O | - | O | O | 수학·삼각 |
| `ARABIC` | O | O | O | O | 수학·삼각 |
| `ASIN` | O | O | O | O | 수학·삼각 |
| `ASINH` | O | O | O | O | 수학·삼각 |
| `ATAN` | O | O | O | O | 수학·삼각 |
| `ATAN2` | O | O | O | O | 수학·삼각 |
| `ATANH` | O | O | O | O | 수학·삼각 |
| `BASE` | O | O | O | O | 수학·삼각 |
| `CEILING.MATH` | O | O | O | O | 수학·삼각 |
| `CEILING.PRECISE` | O | O | O | O | 수학·삼각 |
| `COMBIN` | O | O | O | O | 수학·삼각 |
| `COMBINA` | O | O | O | O | 수학·삼각 |
| `COS` | O | O | O | O | 수학·삼각 |
| `COSH` | O | O | O | O | 수학·삼각 |
| `COT` | O | O | O | O | 수학·삼각 |
| `COTH` | O | O | O | O | 수학·삼각 |
| `CSC` | O | O | O | O | 수학·삼각 |
| `CSCH` | O | O | O | O | 수학·삼각 |
| `DECIMAL` | O | O | O | O | 수학·삼각 |
| `DEGREES` | O | O | O | O | 수학·삼각 |
| `EVEN` | O | O | O | O | 수학·삼각 |
| `EXP` | O | O | O | O | 수학·삼각 |
| `FACT` | O | O | O | O | 수학·삼각 |
| `FACTDOUBLE` | O | O | O | O | 수학·삼각 |
| `FLOOR.MATH` | O | O | O | O | 수학·삼각 |
| `FLOOR.PRECISE` | O | O | O | O | 수학·삼각 |
| `GCD` | O | O | O | O | 수학·삼각 |
| `INT` | O | O | O | O | 수학·삼각 |
| `ISO.CEILING` | O | O | O | - | 수학·삼각 |
| `LCM` | O | O | O | O | 수학·삼각 |
| `LN` | O | O | O | O | 수학·삼각 |
| `LOG` | O | O | O | O | 수학·삼각 |
| `LOG10` | O | O | O | O | 수학·삼각 |
| `MDETERM` | O | O | O | - | 수학·삼각 |
| `MINVERSE` | O | O | O | - | 수학·삼각 |
| `MMULT` | O | O | O | O | 수학·삼각 |
| `MOD` | O | O | O | O | 수학·삼각 |
| `MROUND` | O | O | O | O | 수학·삼각 |
| `MULTINOMIAL` | O | O | O | O | 수학·삼각 |
| `MUNIT` | O | O | O | O | 수학·삼각 |
| `ODD` | O | O | O | O | 수학·삼각 |
| `PERCENTOF` | O | O | O | - | 수학·삼각 |
| `PI` | O | O | O | O | 수학·삼각 |
| `POWER` | O | O | O | O | 수학·삼각 |
| `PRODUCT` | O | O | O | O | 수학·삼각 |
| `QUOTIENT` | O | O | O | O | 수학·삼각 |
| `RADIANS` | O | O | O | O | 수학·삼각 |
| `RAND` | O | O | O | O | 수학·삼각 |
| `RANDARRAY` | O | O | O | - | 수학·삼각 |
| `RANDBETWEEN` | O | O | O | O | 수학·삼각 |
| `ROMAN` | O | O | O | O | 수학·삼각 |
| `ROUND` | O | O | O | O | 수학·삼각 |
| `ROUNDDOWN` | O | O | O | O | 수학·삼각 |
| `ROUNDUP` | O | O | O | O | 수학·삼각 |
| `SEC` | O | O | O | O | 수학·삼각 |
| `SECH` | O | O | O | O | 수학·삼각 |
| `SEQUENCE` | O | O | O | - | 수학·삼각 |
| `SERIESSUM` | O | O | O | O | 수학·삼각 |
| `SIGN` | O | O | O | O | 수학·삼각 |
| `SIN` | O | O | O | O | 수학·삼각 |
| `SINH` | O | O | O | O | 수학·삼각 |
| `SQRT` | O | O | O | O | 수학·삼각 |
| `SQRTPI` | O | O | O | O | 수학·삼각 |
| `SUBTOTAL` | O | O | O | O | 수학·삼각 |
| `SUM` | O | O | O | O | 수학·삼각 |
| `SUMIF` | O | O | O | O | 수학·삼각 |
| `SUMIFS` | O | O | O | O | 수학·삼각 |
| `SUMPRODUCT` | O | O | O | O | 수학·삼각 |
| `SUMSQ` | O | O | O | O | 수학·삼각 |
| `SUMX2MY2` | O | O | O | O | 수학·삼각 |
| `SUMX2PY2` | O | O | O | O | 수학·삼각 |
| `SUMXMY2` | O | O | O | O | 수학·삼각 |
| `TAN` | O | O | O | O | 수학·삼각 |
| `TANH` | O | O | O | O | 수학·삼각 |
| `TRUNC` | O | O | O | O | 수학·삼각 |
| `ACCRINT` | O | O | O | O | 재무 |
| `ACCRINTM` | O | O | O | - | 재무 |
| `AMORDEGRC` | O | O | O | - | 재무 |
| `AMORLINC` | O | O | O | - | 재무 |
| `COUPDAYBS` | O | O | O | - | 재무 |
| `COUPDAYS` | O | O | O | O | 재무 |
| `COUPDAYSNC` | O | O | O | - | 재무 |
| `COUPNCD` | O | O | O | - | 재무 |
| `COUPNUM` | O | O | O | - | 재무 |
| `COUPPCD` | O | O | O | - | 재무 |
| `CUMIPMT` | O | O | O | O | 재무 |
| `CUMPRINC` | O | O | O | O | 재무 |
| `DB` | O | O | O | O | 재무 |
| `DDB` | O | O | O | O | 재무 |
| `DISC` | O | O | O | O | 재무 |
| `DOLLARDE` | O | O | O | O | 재무 |
| `DOLLARFR` | O | O | O | O | 재무 |
| `DURATION` | O | O | O | - | 재무 |
| `EFFECT` | O | O | O | O | 재무 |
| `FV` | O | O | O | O | 재무 |
| `FVSCHEDULE` | O | O | O | O | 재무 |
| `INTRATE` | O | O | O | - | 재무 |
| `IPMT` | O | O | O | O | 재무 |
| `IRR` | O | O | O | O | 재무 |
| `ISPMT` | O | O | O | O | 재무 |
| `MDURATION` | O | O | O | - | 재무 |
| `MIRR` | O | O | O | O | 재무 |
| `NOMINAL` | O | O | O | O | 재무 |
| `NPER` | O | O | O | O | 재무 |
| `NPV` | O | O | O | O | 재무 |
| `ODDFPRICE` | O | O | O | - | 재무 |
| `ODDFYIELD` | O | O | O | - | 재무 |
| `ODDLPRICE` | O | O | O | - | 재무 |
| `ODDLYIELD` | O | O | O | - | 재무 |
| `PDURATION` | O | O | O | O | 재무 |
| `PMT` | O | O | O | O | 재무 |
| `PPMT` | O | O | O | O | 재무 |
| `PRICE` | O | O | O | - | 재무 |
| `PRICEDISC` | O | O | O | O | 재무 |
| `PRICEMAT` | O | O | O | - | 재무 |
| `PV` | O | O | O | O | 재무 |
| `RATE` | O | O | O | O | 재무 |
| `RECEIVED` | O | O | O | - | 재무 |
| `RRI` | O | O | O | O | 재무 |
| `SLN` | O | O | O | O | 재무 |
| `SYD` | O | O | O | O | 재무 |
| `TBILLEQ` | O | O | O | O | 재무 |
| `TBILLPRICE` | O | O | O | O | 재무 |
| `TBILLYIELD` | O | O | O | O | 재무 |
| `VDB` | O | O | O | - | 재무 |
| `XIRR` | O | O | O | O | 재무 |
| `XNPV` | O | O | O | O | 재무 |
| `YIELD` | O | O | O | - | 재무 |
| `YIELDDISC` | O | O | O | - | 재무 |
| `YIELDMAT` | O | O | O | - | 재무 |
| `BESSELI` | O | O | O | O | 공학 |
| `BESSELJ` | O | O | O | O | 공학 |
| `BESSELK` | O | O | O | O | 공학 |
| `BESSELY` | O | O | O | O | 공학 |
| `BIN2DEC` | O | O | O | O | 공학 |
| `BIN2HEX` | O | O | O | O | 공학 |
| `BIN2OCT` | O | O | O | O | 공학 |
| `BITAND` | O | O | O | O | 공학 |
| `BITLSHIFT` | O | O | O | O | 공학 |
| `BITOR` | O | O | O | O | 공학 |
| `BITRSHIFT` | O | O | O | O | 공학 |
| `BITXOR` | O | O | O | O | 공학 |
| `COMPLEX` | O | O | O | O | 공학 |
| `CONVERT` | O | O | O | O | 공학 |
| `DEC2BIN` | O | O | O | O | 공학 |
| `DEC2HEX` | O | O | O | O | 공학 |
| `DEC2OCT` | O | O | O | O | 공학 |
| `DELTA` | O | O | O | O | 공학 |
| `ERF` | O | O | O | O | 공학 |
| `ERF.PRECISE` | O | O | O | - | 공학 |
| `ERFC` | O | O | O | O | 공학 |
| `ERFC.PRECISE` | O | O | O | - | 공학 |
| `GESTEP` | O | O | O | O | 공학 |
| `HEX2BIN` | O | O | O | O | 공학 |
| `HEX2DEC` | O | O | O | O | 공학 |
| `HEX2OCT` | O | O | O | O | 공학 |
| `IMABS` | O | O | O | O | 공학 |
| `IMAGINARY` | O | O | O | O | 공학 |
| `IMARGUMENT` | O | O | O | O | 공학 |
| `IMCONJUGATE` | O | O | O | O | 공학 |
| `IMCOS` | O | O | O | O | 공학 |
| `IMCOSH` | O | O | O | O | 공학 |
| `IMCOT` | O | O | O | O | 공학 |
| `IMCSC` | O | O | O | O | 공학 |
| `IMCSCH` | O | O | O | O | 공학 |
| `IMDIV` | O | O | O | O | 공학 |
| `IMEXP` | O | O | O | O | 공학 |
| `IMLN` | O | O | O | O | 공학 |
| `IMLOG10` | O | O | O | O | 공학 |
| `IMLOG2` | O | O | O | O | 공학 |
| `IMPOWER` | O | O | O | O | 공학 |
| `IMPRODUCT` | O | O | O | O | 공학 |
| `IMREAL` | O | O | O | O | 공학 |
| `IMSEC` | O | O | O | O | 공학 |
| `IMSECH` | O | O | O | O | 공학 |
| `IMSIN` | O | O | O | O | 공학 |
| `IMSINH` | O | O | O | O | 공학 |
| `IMSQRT` | O | O | O | O | 공학 |
| `IMSUB` | O | O | O | O | 공학 |
| `IMSUM` | O | O | O | O | 공학 |
| `IMTAN` | O | O | O | O | 공학 |
| `OCT2BIN` | O | O | O | O | 공학 |
| `OCT2DEC` | O | O | O | O | 공학 |
| `OCT2HEX` | O | O | O | O | 공학 |
| `ARRAYTOTEXT` | O | O | O | - | 텍스트 |
| `ASC` | O | O | O | - | 텍스트 |
| `BAHTTEXT` | - | - | O | - | 텍스트 |
| `CHAR` | O | O | O | O | 텍스트 |
| `CLEAN` | O | O | O | O | 텍스트 |
| `CODE` | O | O | O | O | 텍스트 |
| `CONCAT` | O | O | O | O | 텍스트 |
| `CONCATENATE` | O | O | O | O | 텍스트 |
| `DBCS` | O | - | O | - | 텍스트 |
| `DETECTLANGUAGE` | - | - | - | - | 텍스트 |
| `DOLLAR` | O | O | O | O | 텍스트 |
| `EXACT` | O | O | O | O | 텍스트 |
| `FIND` | O | O | O | O | 텍스트 |
| `FINDB` | O | O | O | - | 텍스트 |
| `FIXED` | O | O | O | O | 텍스트 |
| `JIS` | - | - | - | - | 텍스트 |
| `LEFT` | O | O | O | O | 텍스트 |
| `LEFTB` | O | O | O | - | 텍스트 |
| `LEN` | O | O | O | O | 텍스트 |
| `LENB` | O | O | O | - | 텍스트 |
| `LOWER` | O | O | O | O | 텍스트 |
| `MID` | O | O | O | O | 텍스트 |
| `MIDB` | O | O | O | - | 텍스트 |
| `NUMBERVALUE` | O | O | O | O | 텍스트 |
| `PHONETIC` | - | - | - | - | 텍스트 |
| `PROPER` | O | O | O | O | 텍스트 |
| `REGEXEXTRACT` | O | O | O | - | 텍스트 |
| `REGEXREPLACE` | O | O | O | - | 텍스트 |
| `REGEXTEST` | O | O | O | - | 텍스트 |
| `REPLACE` | O | O | O | O | 텍스트 |
| `REPLACEB` | O | O | O | - | 텍스트 |
| `REPT` | O | O | O | O | 텍스트 |
| `RIGHT` | O | O | O | O | 텍스트 |
| `RIGHTB` | O | O | O | - | 텍스트 |
| `SEARCH` | O | O | O | O | 텍스트 |
| `SEARCHB` | O | O | O | - | 텍스트 |
| `SUBSTITUTE` | O | O | O | O | 텍스트 |
| `T` | O | O | O | O | 텍스트 |
| `TEXT` | O | O | O | O | 텍스트 |
| `TEXTAFTER` | O | O | O | - | 텍스트 |
| `TEXTBEFORE` | O | O | O | - | 텍스트 |
| `TEXTJOIN` | O | O | O | O | 텍스트 |
| `TEXTSPLIT` | O | O | O | - | 텍스트 |
| `TRANSLATE` | - | - | - | - | 텍스트 |
| `TRIM` | O | O | O | O | 텍스트 |
| `UNICHAR` | O | O | O | O | 텍스트 |
| `UNICODE` | O | O | O | O | 텍스트 |
| `UPPER` | O | O | O | O | 텍스트 |
| `VALUE` | O | O | O | O | 텍스트 |
| `VALUETOTEXT` | O | O | O | - | 텍스트 |
| `BETADIST` | O | O | O | O | 호환성 |
| `BETAINV` | O | O | O | O | 호환성 |
| `BINOMDIST` | O | O | O | O | 호환성 |
| `CEILING` | O | O | O | O | 호환성 |
| `CHIDIST` | O | O | O | O | 호환성 |
| `CHIINV` | O | O | O | O | 호환성 |
| `CHITEST` | O | O | O | O | 호환성 |
| `CONFIDENCE` | O | O | O | - | 호환성 |
| `COVAR` | O | O | O | O | 호환성 |
| `CRITBINOM` | O | O | O | O | 호환성 |
| `EXPONDIST` | O | O | O | O | 호환성 |
| `FDIST` | O | O | O | O | 호환성 |
| `FINV` | O | O | O | O | 호환성 |
| `FLOOR` | O | O | O | O | 호환성 |
| `FTEST` | O | O | O | O | 호환성 |
| `GAMMADIST` | O | O | O | O | 호환성 |
| `GAMMAINV` | O | O | O | O | 호환성 |
| `HYPGEOMDIST` | O | O | O | O | 호환성 |
| `LOGINV` | O | O | O | O | 호환성 |
| `LOGNORMDIST` | O | O | O | O | 호환성 |
| `MODE` | O | O | O | - | 호환성 |
| `NEGBINOMDIST` | O | O | O | O | 호환성 |
| `NORM.INV` | O | O | O | O | 호환성 |
| `NORMDIST` | O | O | O | O | 호환성 |
| `NORMSDIST` | O | O | O | O | 호환성 |
| `NORMSINV` | O | O | O | O | 호환성 |
| `PERCENTILE` | O | O | O | - | 호환성 |
| `PERCENTRANK` | O | O | O | - | 호환성 |
| `POISSON` | O | O | O | - | 호환성 |
| `QUARTILE` | O | O | O | - | 호환성 |
| `RANK` | O | O | O | - | 호환성 |
| `STDEV` | O | O | O | - | 호환성 |
| `STDEVP` | O | O | O | O | 호환성 |
| `TDIST` | O | O | O | O | 호환성 |
| `TINV` | O | O | O | O | 호환성 |
| `TTEST` | O | O | O | O | 호환성 |
| `VAR` | O | O | O | - | 호환성 |
| `VARP` | O | O | O | O | 호환성 |
| `WEIBULL` | O | O | O | - | 호환성 |
| `ZTEST` | O | O | O | O | 호환성 |
| `ADDRESS` | O | O | O | - | 조회·참조 |
| `AREAS` | O | O | O | - | 조회·참조 |
| `CHOOSE` | O | O | O | O | 조회·참조 |
| `CHOOSECOLS` | O | O | O | O | 조회·참조 |
| `CHOOSEROWS` | O | O | O | O | 조회·참조 |
| `COLUMN` | O | O | O | O | 조회·참조 |
| `COLUMNS` | O | O | O | O | 조회·참조 |
| `DROP` | O | O | O | O | 조회·참조 |
| `EXPAND` | O | O | O | O | 조회·참조 |
| `FILTER` | O | O | O | - | 조회·참조 |
| `FORMULATEXT` | O | O | O | - | 조회·참조 |
| `GETPIVOTDATA` | - | - | O | - | 조회·참조 |
| `GROUPBY` | - | - | O | - | 조회·참조 |
| `HLOOKUP` | O | O | O | O | 조회·참조 |
| `HSTACK` | O | O | O | O | 조회·참조 |
| `HYPERLINK` | O | O | O | - | 조회·참조 |
| `IMAGE` | - | - | O | - | 조회·참조 |
| `INDEX` | O | O | O | O | 조회·참조 |
| `INDIRECT` | O | O | O | - | 조회·참조 |
| `LOOKUP` | O | O | O | O | 조회·참조 |
| `MATCH` | O | O | O | O | 조회·참조 |
| `OFFSET` | O | O | O | - | 조회·참조 |
| `PIVOTBY` | - | - | - | - | 조회·참조 |
| `ROW` | O | O | O | O | 조회·참조 |
| `ROWS` | O | O | O | O | 조회·참조 |
| `RTD` | - | - | - | - | 조회·참조 |
| `SORT` | O | O | O | O | 조회·참조 |
| `SORTBY` | O | O | O | - | 조회·참조 |
| `TAKE` | O | O | O | O | 조회·참조 |
| `TOCOL` | O | O | O | - | 조회·참조 |
| `TOROW` | O | O | O | - | 조회·참조 |
| `TRANSPOSE` | O | O | O | O | 조회·참조 |
| `TRIMRANGE` | O | O | - | - | 조회·참조 |
| `UNIQUE` | O | O | O | O | 조회·참조 |
| `VLOOKUP` | O | O | O | O | 조회·참조 |
| `VSTACK` | O | O | O | O | 조회·참조 |
| `WRAPCOLS` | O | O | O | - | 조회·참조 |
| `WRAPROWS` | O | O | O | - | 조회·참조 |
| `XLOOKUP` | O | O | O | - | 조회·참조 |
| `XMATCH` | O | O | O | - | 조회·참조 |
| `DATE` | O | O | O | O | 날짜·시간 |
| `DATEDIF` | O | O | O | O | 날짜·시간 |
| `DATEVALUE` | O | O | O | O | 날짜·시간 |
| `DAY` | O | O | O | O | 날짜·시간 |
| `DAYS` | O | O | O | O | 날짜·시간 |
| `DAYS360` | O | O | O | O | 날짜·시간 |
| `EDATE` | O | O | O | O | 날짜·시간 |
| `EOMONTH` | O | O | O | O | 날짜·시간 |
| `HOUR` | O | O | O | O | 날짜·시간 |
| `ISOWEEKNUM` | O | O | O | O | 날짜·시간 |
| `MINUTE` | O | O | O | O | 날짜·시간 |
| `MONTH` | O | O | O | O | 날짜·시간 |
| `NETWORKDAYS` | O | O | O | O | 날짜·시간 |
| `NETWORKDAYS.INTL` | O | O | O | O | 날짜·시간 |
| `NOW` | O | O | O | O | 날짜·시간 |
| `SECOND` | O | O | O | O | 날짜·시간 |
| `TIME` | O | O | O | O | 날짜·시간 |
| `TIMEVALUE` | O | O | O | O | 날짜·시간 |
| `TODAY` | O | O | O | O | 날짜·시간 |
| `WEEKDAY` | O | O | O | O | 날짜·시간 |
| `WEEKNUM` | O | O | O | O | 날짜·시간 |
| `WORKDAY` | O | O | O | O | 날짜·시간 |
| `WORKDAY.INTL` | O | O | O | O | 날짜·시간 |
| `YEAR` | O | O | O | O | 날짜·시간 |
| `YEARFRAC` | O | O | O | O | 날짜·시간 |
| `CELL` | O | O | O | - | 정보 |
| `ERROR.TYPE` | O | O | O | O | 정보 |
| `INFO` | O | O | - | - | 정보 |
| `ISBLANK` | O | O | O | O | 정보 |
| `ISERR` | O | O | O | O | 정보 |
| `ISERROR` | O | O | O | O | 정보 |
| `ISEVEN` | O | O | O | O | 정보 |
| `ISFORMULA` | O | O | O | - | 정보 |
| `ISLOGICAL` | O | O | O | O | 정보 |
| `ISNA` | O | O | O | O | 정보 |
| `ISNONTEXT` | O | O | O | O | 정보 |
| `ISNUMBER` | O | O | O | O | 정보 |
| `ISODD` | O | O | O | O | 정보 |
| `ISOMITTED` | O | O | - | - | 정보 |
| `ISREF` | O | O | O | - | 정보 |
| `ISTEXT` | O | O | O | O | 정보 |
| `N` | O | O | O | O | 정보 |
| `NA` | O | O | O | O | 정보 |
| `SHEET` | O | O | O | - | 정보 |
| `SHEETS` | O | O | O | - | 정보 |
| `STOCKHISTORY` | - | - | - | - | 정보 |
| `TYPE` | O | O | O | O | 정보 |
| `AND` | O | O | O | O | 논리 |
| `BYCOL` | O | O | O | - | 논리 |
| `BYROW` | O | O | O | - | 논리 |
| `FALSE` | O | O | O | O | 논리 |
| `IF` | O | O | O | O | 논리 |
| `IFERROR` | O | O | O | O | 논리 |
| `IFNA` | O | O | O | O | 논리 |
| `IFS` | O | O | O | O | 논리 |
| `LAMBDA` | O | O | O | - | 논리 |
| `LET` | O | O | O | - | 논리 |
| `MAKEARRAY` | O | O | O | - | 논리 |
| `MAP` | O | O | O | - | 논리 |
| `NOT` | O | O | O | O | 논리 |
| `OR` | O | O | O | O | 논리 |
| `REDUCE` | O | O | O | - | 논리 |
| `SCAN` | O | O | O | - | 논리 |
| `SWITCH` | O | O | O | O | 논리 |
| `TRUE` | O | O | O | O | 논리 |
| `XOR` | O | O | O | O | 논리 |
| `DAVERAGE` | O | O | O | O | 데이터베이스 |
| `DCOUNT` | O | O | O | O | 데이터베이스 |
| `DCOUNTA` | O | O | O | O | 데이터베이스 |
| `DGET` | O | O | O | O | 데이터베이스 |
| `DMAX` | O | O | O | O | 데이터베이스 |
| `DMIN` | O | O | O | O | 데이터베이스 |
| `DPRODUCT` | O | O | O | O | 데이터베이스 |
| `DSTDEV` | O | O | O | O | 데이터베이스 |
| `DSTDEVP` | O | O | O | O | 데이터베이스 |
| `DSUM` | O | O | O | O | 데이터베이스 |
| `DVAR` | O | O | O | O | 데이터베이스 |
| `DVARP` | O | O | O | O | 데이터베이스 |
| `CUBEKPIMEMBER` | - | - | - | - | 큐브 |
| `CUBEMEMBER` | - | - | - | - | 큐브 |
| `CUBEMEMBERPROPERTY` | - | - | - | - | 큐브 |
| `CUBERANKEDMEMBER` | - | - | - | - | 큐브 |
| `CUBESET` | - | - | - | - | 큐브 |
| `CUBESETCOUNT` | - | - | - | - | 큐브 |
| `CUBEVALUE` | - | - | - | - | 큐브 |
| `CALL` | - | - | - | - | 추가 기능·자동화 |
| `EUROCONVERT` | - | - | O | - | 추가 기능·자동화 |
| `REGISTER.ID` | - | - | - | - | 추가 기능·자동화 |
| `ENCODEURL` | O | O | O | - | 웹 |
| `FILTERXML` | - | - | O | - | 웹 |
| `WEBSERVICE` | - | - | - | - | 웹 |
