# Compilation Baseline

## Method

The artifacts below were collected from the current release CLI with:

```text
GALFUS_DEBUG_BYTECODE_STATS=1 ./target/release/galfus-cli run benchmark/<case>.gfs
```

The full deterministic structural reports and the benchmark result emitted by
each source are retained in:

- `compilation-baseline-fib.jsonlog`
- `compilation-baseline-matrix4.jsonlog`
- `compilation-baseline-tasks.jsonlog`

The reported package size is the encoded `PackageImage` size. Counts include
all modules transitively compiled for each benchmark, not only the entry
module.

## Package totals

| Case | Bytes | Modules | Functions | Instructions | Constants | Types | Layouts | Frame registers | Local calls | Import calls | Dynamic calls | Branches | Future creations |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Fib | 7,582 | 7 | 38 | 1,673 | 39 | 60 | 3 | 782 | 38 | 20 | 27 | 151 | 5 |
| Matrix4 | 9,060 | 7 | 39 | 1,929 | 58 | 61 | 3 | 1,015 | 36 | 21 | 29 | 152 | 5 |
| Tasks | 9,624 | 8 | 52 | 2,015 | 60 | 73 | 4 | 979 | 39 | 28 | 41 | 193 | 10 |

## Hot functions

| Case | Function | Instructions | Frame registers | Local calls | Import calls | Dynamic calls | Branches |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Fib | `fib` | 11 | 8 | 2 | 0 | 0 | 2 |
| Matrix4 | `matrix4` | 263 | 239 | 0 | 1 | 2 | 3 |
| Tasks | `worker` | 145 | 79 | 0 | 4 | 6 | 18 |
| Tasks | `main` | 149 | 79 | 0 | 12 | 8 | 24 |

## Observed program results

| Case | Result | Reported script time |
| --- | ---: | ---: |
| Fib | 9,227,465 | 1,110 ms |
| Matrix4 | 1,999,214,662,944,230 | 1,242 ms |
| Tasks | 3,793,104 | 151 ms |

These are single correctness runs, not performance medians. Multi-sample
release comparisons belong to Phase G.
