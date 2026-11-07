// EasyOffice extension (print-pages-v1): the shell prints the document, so it asks the editor for the
// pages in small batches instead of receiving every page SVG at once. Each batch is rendered with the
// print profile under portable metrics, the same way the editor's own print path does, and carries
// the paper size in millimetres so the shell can write one @page rule per distinct size.
import { createPrintPage, type PrintPageInfo } from '../command/print-pages.ts';

/** Pages per request: small enough to bound one message, large enough to keep the round trips few. */
export const PRINT_BATCH_MAX_PAGES = 10;

export interface PrintBatchSource {
  readonly pageCount: number;
  withPortableMetrics<T>(operation: () => T): T;
  renderPageSvgWithProfile(page: number, profile: 'print'): string;
  getPageInfo(page: number): PrintPageInfo;
}

export interface PrintBatchPage {
  svg: string;
  widthMm: number;
  heightMm: number;
}

export interface PrintBatchResult {
  pageCount: number;
  pages: PrintBatchPage[];
}

export interface PrintBatchRange {
  from: number;
  count: number;
}

function asNonNegativeInteger(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new Error(`${label} must be a non-negative safe integer`);
  }
  return value as number;
}

/** Validates the request fields; `count` is limited to 1..=PRINT_BATCH_MAX_PAGES. */
export function parsePrintBatchRange(params: Record<string, unknown>): PrintBatchRange {
  const unknown = Object.keys(params).filter(key => key !== 'from' && key !== 'count');
  if (unknown.length > 0) throw new Error(`getPrintPages params contains unknown field: ${unknown[0]}`);
  const from = asNonNegativeInteger(params.from ?? 0, 'from');
  const count = asNonNegativeInteger(params.count ?? PRINT_BATCH_MAX_PAGES, 'count');
  if (count < 1 || count > PRINT_BATCH_MAX_PAGES) {
    throw new Error(`count must be in 1..=${PRINT_BATCH_MAX_PAGES}`);
  }
  return { from, count };
}

/** Renders pages `from..from+count` (clamped to the document). An out-of-range start is an error. */
export function renderPrintBatch(source: PrintBatchSource, from: number, count: number): PrintBatchResult {
  const pageCount = source.pageCount;
  if (from >= pageCount && !(from === 0 && pageCount === 0)) {
    throw new Error('page is out of range');
  }
  const end = Math.min(pageCount, from + count);
  const pages = source.withPortableMetrics(() => {
    const batch: PrintBatchPage[] = [];
    for (let index = from; index < end; index++) {
      const svg = source.renderPageSvgWithProfile(index, 'print');
      const page = createPrintPage(svg, source.getPageInfo(index), index);
      batch.push({ svg, widthMm: page.widthMm, heightMm: page.heightMm });
    }
    return batch;
  });
  return { pageCount, pages };
}
