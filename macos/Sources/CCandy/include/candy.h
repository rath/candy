#ifndef CANDY_H
#define CANDY_H
#include <stdint.h>
#include <stddef.h>
// Handles are owned. Free once, after all calls using them finish. Documents
// require serialized access; independent documents/images can run concurrently.
// Input strings are NUL-terminated filesystem paths, borrowed for the call.
// Arrays are borrowed, correctly aligned, and valid for count elements. NULL is
// allowed only for an empty array. Errors are owned strings, freed with error_free.
// Every fallible call initializes *error; a NULL error means success.
// Returned preview/region buffers live until their enclosing result is freed.
// Functions do no UI work. Call decode/render/OCR/save off the main thread.
typedef struct CandyDocument CandyDocument;
typedef struct CandyImage CandyImage;
typedef struct { uint32_t x, y, width, height; } CandyRect;
typedef struct { float x, y; } CandyPoint;
// quality: 0 = format default, 1–100 = lossy quality.
// webp_mode: 0 = default, 1 = lossless, 2 = lossy. Only WebP accepts a mode.
// WebP quality requires lossy mode. Invalid options fail without writing.
typedef struct { uint8_t quality, webp_mode; } CandySaveOptions;
typedef struct { uint32_t width, height; uint64_t state_id, revision; uint8_t dirty, can_undo, can_redo; } CandyState;
// Tightly packed, premultiplied RGBA8 sRGB; top-left origin. At most 4096 per side.
typedef struct { uint32_t width, height; size_t length; uint8_t *data; } CandyPreview;
typedef struct { size_t count; CandyRect *data; } CandyRegions;
void candy_error_free(char *error);
CandyDocument *candy_document_open(const char *path, char **error);
void candy_document_free(CandyDocument *document);
CandyState candy_document_state(CandyDocument *document, char **error);
uint8_t candy_document_history(CandyDocument *document, uint8_t redo, char **error);
uint8_t candy_document_resize(CandyDocument *document, uint32_t width, uint32_t height, char **error);
// Quarter turn; non-zero `clockwise` turns right, zero turns left.
uint8_t candy_document_rotate(CandyDocument *document, uint8_t clockwise, char **error);
uint8_t candy_document_flatten(CandyDocument *document, uint8_t r, uint8_t g, uint8_t b, char **error);
uint8_t candy_document_mosaic(CandyDocument *document, const CandyRect *rects, size_t count, uint32_t block, char **error);
uint8_t candy_document_highlight(CandyDocument *document, const CandyPoint *points, size_t count, float width, uint8_t r, uint8_t g, uint8_t b, uint8_t a, char **error);
CandyDocument *candy_document_extract(CandyDocument *document, CandyRect rect, char **error);
uint8_t candy_document_save(CandyDocument *document, const char *path, char **error);
uint8_t candy_document_save_with_options(CandyDocument *document, const char *path, CandySaveOptions options, char **error);
CandyPreview *candy_document_preview(CandyDocument *document, char **error);
void candy_preview_free(CandyPreview *preview);
CandyImage *candy_document_snapshot(CandyDocument *document, char **error);
void candy_image_free(CandyImage *image);
CandyRegions *candy_image_detect(const CandyImage *image, char **error);
void candy_regions_free(CandyRegions *regions);
#endif
