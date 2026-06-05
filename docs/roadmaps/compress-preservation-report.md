# Compress Preservation and Token Reduction Report

This report captures the current baseline evidence for the frozen `compress`
implementation in rUvOS.

The report uses representative fixtures shaped like the current benchmark
inputs and records both byte-level and token-level reduction. It is intended as
audit evidence, not as a new runtime feature.

## Measurement Method

- Input path: `ruvos compress`
- Content kinds: `json`, `log`, `code`
- Output: CLI JSON summary with `original_bytes`, `compressed_bytes`,
  `bytes_saved`, `compression_ratio`, `tokens_before`, and `tokens_after`
- Benchmark timing reference: Criterion baseline already recorded in
  [compress-baseline-checklist.md](./compress-baseline-checklist.md)

## Evidence

| Kind | Original bytes | Compressed bytes | Bytes saved | Compression ratio | Tokens before | Tokens after | Token reduction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| JSON | 9,711 | 840 | 8,871 | 0.0865 | 403 | 111 | 72.46% |
| Log | 11,098 | 3,088 | 8,010 | 0.2782 | 1,602 | 517 | 67.73% |
| Code | 5,205 | 1,653 | 3,552 | 0.3176 | 882 | 362 | 58.96% |

## Representative CLI Outputs

### JSON

- `kind`: `json`
- `changed`: `true`
- `original_ref`: `9e0dfb7e08b1bc1a99c1614d`

### Log

- `kind`: `log`
- `changed`: `true`
- `original_ref`: `0b9bc7d3d58571c9e5867398`

### Code

- `kind`: `code`
- `changed`: `true`
- `original_ref`: `f6a2ba713f217ce106e61903`

## Baseline Timing Reference

Criterion benchmark timing captured in the frozen checklist:

- `compress_json`: `108.07 µs` to `108.67 µs`
- `compress_log`: `48.975 µs` to `49.140 µs`
- `compress_code`: `18.043 µs` to `18.392 µs`

## Interpretation

- JSON currently gives the strongest token reduction because array thinning and
  signal preservation cut repeated records aggressively.
- Log/text compression is the next strongest path because it preserves error
  clusters and nearby context while trimming repetitive lines.
- Code compression is intentionally conservative and preserves structural
  boundaries, so its token reduction is smaller but the output remains useful.

## Related Files

- [Compress Baseline Checklist](./compress-baseline-checklist.md)
- [Compress Implementation Map](./compress-implementation-map.md)
