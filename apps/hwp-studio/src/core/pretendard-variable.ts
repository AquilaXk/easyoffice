/**
 * Pretendard ships as one variable woff2 (assets/fonts/PretendardVariable.woff2, PD-298). The upstream
 * font catalog still names one static file per weight (`fonts/Pretendard-Bold.woff2`, family "Pretendard Bold"),
 * so each catalog name is served by the variable file with a FontFace weight descriptor pinned to that weight.
 * A single value in the descriptor clamps the requested weight to it, so the family renders at that weight
 * only, like the static file did. The family "Pretendard" itself is the Regular weight (400).
 */

export const PRETENDARD_VARIABLE_FILE = 'fonts/PretendardVariable.woff2';

export const PRETENDARD_STATIC_WEIGHTS: ReadonlyMap<string, number> = new Map([
  ['fonts/Pretendard-Thin.woff2', 100],
  ['fonts/Pretendard-ExtraLight.woff2', 200],
  ['fonts/Pretendard-Light.woff2', 300],
  ['fonts/Pretendard-Regular.woff2', 400],
  ['fonts/Pretendard-Medium.woff2', 500],
  ['fonts/Pretendard-SemiBold.woff2', 600],
  ['fonts/Pretendard-Bold.woff2', 700],
  ['fonts/Pretendard-ExtraBold.woff2', 800],
  ['fonts/Pretendard-Black.woff2', 900],
]);

export interface VariableFace {
  /** The file that serves the catalog entry. */
  file: string;
  /** FontFace `weight` descriptor: the one weight the family renders at. */
  weight: string;
}

/** The variable face for a catalog file name, or null when the file is not one of the nine static Pretendard files. */
export function variableFaceFor(file: string): VariableFace | null {
  const weight = PRETENDARD_STATIC_WEIGHTS.get(file);
  return weight === undefined ? null : { file: PRETENDARD_VARIABLE_FILE, weight: String(weight) };
}
