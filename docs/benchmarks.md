# Measured performance and quality

Engineering measurements on one development workstation, not fleet percentiles or a promise of
instant recognition: Windows 11 x64, Intel Core i5-14400F (6 performance and 4 efficiency cores,
16 logical processors), 16 GiB RAM, NVIDIA GeForce RTX 5060 with about 8 GiB of dedicated memory.
Low-core laptops, older CPUs, AMD and Intel GPUs, ARM64, Linux and macOS have not been
benchmarked.

## Speech recognition

Source: `tests/fixtures/asr/jfk.wav`, public-domain speech, mono 16 kHz, 11.0 seconds, and the
same clip cut to 3 seconds. Models come from the pinned revision through Onda's own verified
downloader. Six CPU threads, greedy decoding. Measured in October 2026 with the current
optimized build, `audio_ctx` sized to the clip and the warm start.

| Model | Clip | CPU inference |
| --- | --- | ---: |
| tiny | 3 s | 89 ms |
| tiny | 11 s | 221 ms |
| small Q5_1 | 3 s | 924 ms |
| small Q5_1 | 11 s | 1,354 ms |

Latency from the end of speech to text, with the worker started at recording start and an idle
gap as long as the clip: 85 ms (tiny, 3 s, Auto on the CPU), 211 ms (small Q5_1, 3 s, Auto on
the GPU) and 383 ms (small Q5_1, 11 s, Auto on the GPU).

Worker priority matters after the machine has idled. At below-normal priority Windows moved the
windowless worker onto efficiency cores: warm tiny inference ranged 68–1,197 ms and small Q5_1
397–1,754 ms. At normal priority, which Onda uses, the same runs took 69–149 ms and 378–436 ms.

The first Vulkan inference on a machine prepares the driver's shader and pipeline caches. In an
earlier measurement (October 2026, before the CPU build was optimized), tiny.en on Vulkan took
9.75 s the first time and about 1.0 s afterwards; base.en Q5_1 took 25.2 s the first time, under
build contention, and 1.5–2.1 s afterwards. That first-use cost is real user-visible latency.

## Dictionary and text quality

`npm run quality` runs the deterministic quality fixtures (replacements, Unicode and emoji,
meaningful repetition, numbers, URLs, emails, explicit corrections, code, hostile instructions in
the source, refusals and unwanted Markdown) and times a 10,000-entry vocabulary index. On
2026-11-12 the index built in 68 ms ([VERIFICATION.md](../VERIFICATION.md)). Retrieval is timed
with an exact synthetic query, not a worst-case fuzzy search or an accuracy result.

Local AI polish timings on a CPU-only machine are in [local-models.md](local-models.md).

## Reproduction

```powershell
cargo run --locked --release -p onda-core --bin onda-quality
.\scripts\asr-benchmark.ps1 -ModelId whisper-tiny-en
.\scripts\asr-benchmark.ps1 -SkipBuild -ModelId whisper-small-en-q5
.\scripts\asr-benchmark.ps1 -SkipBuild -ExecutionPreference gpu -CheckVulkan
```

The ASR script checks the fixture, installs and verifies the model outside the repository,
reports the hardware and each run, then runs the golden tests. GPU workers must first be built
with `scripts/prepare.ps1 -Release -WithVulkan` ([asr.md](asr.md)). Report first-use and repeated
measurements, and whether other work is loading the computer.

Not yet measured: a latency distribution from shortcut to text in real apps, live cloud polish
latency, an accuracy corpus on ordinary laptops, and any fleet percentile.
