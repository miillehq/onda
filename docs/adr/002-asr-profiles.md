# ADR 002: Generic CPU Whisper baseline and optional Vulkan

Status: accepted. Date: 2026-10-03.

Whisper.cpp via whisper-rs has permissive MIT licensing, quantized small models and a usable general CPU baseline. ONNX/Parakeet is promising, but provider support, memory footprint and model/license variants require another validated integration. CUDA imposes NVIDIA-only packaging. DirectML requires an ONNX graph/provider stack. Vulkan covers multiple Windows GPU vendors and can live entirely in an optional isolated worker.

Choose a generic x64 CPU build (no native-host AVX assumptions), with a separate Vulkan executable and real fallback tests. Fast defaults to tiny; Balanced and Quality select quantized base/small with language and RAM constraints. The measured eleven-second English fixture costs roughly three seconds on generic CPU tiny and eight seconds on base; it would be misleading to default to a larger model based only on advertised quality. GPU driver shader caches produce materially different first-use and later latency, recorded separately in benchmarks.md.

Workers reload models per job. This provides hard cancellation, crash isolation and almost zero idle model residency, at a measured load cost around 0.14–0.2 seconds for tiny on this hardware. A future resident worker can be considered after measuring safe cancellation and low-memory behavior, without changing the frontend contract.

Update (2026-11-13): On x86-64 a second CPU worker built with AVX2 runs when the CPU has AVX2, FMA, F16C and BMI2, and the baseline worker remains the retry; ARM builds have only the baseline worker. Vulkan is also used on Linux; on macOS the worker is built with Metal and falls back to the CPU. The CPU timings above came from a build that compiled whisper.cpp without optimization; [benchmarks.md](../benchmarks.md) has the measurements after the fix. Workers still serve one recording each, but start and load the model when recording starts, so loading overlaps speech ([asr.md](../asr.md)). The third profile is called Quality in the interface and `bestQuality` in settings.
