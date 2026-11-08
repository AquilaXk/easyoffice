#ifndef EASYOFFICE_QUICKLOOK_FFI_H
#define EASYOFFICE_QUICKLOOK_FFI_H

// C ABI of apps/quicklook/rust (easyoffice-quicklook-ffi). Every function fails closed: a status
// other than 0 comes with no buffer. A returned buffer is released with eo_ql_free_bytes.

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct EasyOfficeQuickLookBytes {
  uint8_t *ptr;
  size_t len;
} EasyOfficeQuickLookBytes;

typedef struct EasyOfficeQuickLookRenderResult {
  int32_t status;
  EasyOfficeQuickLookBytes bytes;
  uint32_t page_count;
  // First page size in CSS pixels (96 per inch).
  double width;
  double height;
} EasyOfficeQuickLookRenderResult;

typedef struct EasyOfficeQuickLookThumbnailResult {
  int32_t status;
  EasyOfficeQuickLookBytes bytes;
  uint32_t width;
  uint32_t height;
} EasyOfficeQuickLookThumbnailResult;

// Up to 64 pages as one PDF.
EasyOfficeQuickLookRenderResult eo_ql_render_preview_pdf(const uint8_t *data, size_t len);
// The first page as PNG, longer side max_dimension pixels (0 = the largest size).
EasyOfficeQuickLookRenderResult eo_ql_render_first_page_png(const uint8_t *data, size_t len, uint32_t max_dimension);
// The preview picture stored in the file, as stored (PNG, BMP or GIF).
EasyOfficeQuickLookThumbnailResult eo_ql_extract_embedded_thumbnail(const uint8_t *data, size_t len);
void eo_ql_free_bytes(uint8_t *ptr, size_t len);

#ifdef __cplusplus
}
#endif

#endif
