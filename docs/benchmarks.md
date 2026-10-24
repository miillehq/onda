# Measured performance and quality

These are reproducible engineering measurements from 3 October 2026, not fleet
percentiles or claims of instant recognition. One development workstation was
available: Windows x64, Intel Core i5-14400F (10 physical/16 logical cores),
16 GiB RAM, NVIDIA GeForce RTX 5060 with approximately 8 GiB dedicated memory.
Other processes and development builds were present; contention is identified
below. Ordinary low-core laptops, older CPUs, AMD/Intel GPUs and Windows ARM64
have not been benchmarked.

## Real local recognition

The source is `tests/fixtures/asr/jfk.wav`: actual public-domain speech, mono
16 kHz PCM16, 176,000 samples, 11.000 seconds. Its checksum and rights/provenance
are recorded alongside the fixture. Models were downloaded by Eagle's real
HTTPS/resume/checksum manager outside Git from pinned revision
`5359861c739e955e79d9a303bcbc70fb988958b1`.

The ASR build uses `whisper-rs 0.16.0`, `whisper-rs-sys 0.15.0`, bundled
`whisper.cpp 1.8.3`, greedy decoding, language `en`, no vocabulary prompt, and six
CPU inference threads. Generic x64 disables native tuning, AVX, AVX2, AVX512,
FMA, F16C, BMI2 and SSE4.2 in the inspected CMake caches. Native C++ is optimized
even in the Rust development build. Each repetition starts a fresh worker, so
total latency includes process setup, audio transfer, model load and teardown.
Vulkan totals additionally include an isolated device probe.

| Model/backend | Repetition | Model load ms | Inference ms | Total ms |
| --- | ---: | ---: | ---: | ---: |
| tiny.en CPU | 1 | 137 | 2,761 | 3,079 |
| tiny.en CPU | 2 | 158 | 2,712 | 2,954 |
| tiny.en CPU | 3 | 141 | 2,728 | 2,957 |
| base.en Q5_1 CPU | 1 | 113 | 8,133 | 8,363 |
| base.en Q5_1 CPU | 2 | 125 | 8,551 | 8,761 |
| base.en Q5_1 CPU | 3 | 117 | 7,839 | 8,046 |
| tiny.en Vulkan | First shader use | 194 | 8,782 | 9,750 |
| tiny.en Vulkan | Warm driver cache 1 | 170 | 105 | 1,019 |
| tiny.en Vulkan | Warm driver cache 2 | 176 | 101 | 1,009 |
| base.en Q5_1 Vulkan | First model/shader use, contended | 2,062 | 13,796 | 25,224 |
| base.en Q5_1 Vulkan | Warm driver cache 1 | 447 | 224 | 2,106 |
| base.en Q5_1 Vulkan | Warm driver cache 2 | 264 | 196 | 1,543 |

The initial tiny CPU median total was 2.957 seconds, approximately 0.269 times
audio duration. Balanced base Q5_1 median total was 8.363 seconds, approximately
0.760 times audio duration. The two warm tiny Vulkan runs averaged 1.014 seconds
total. Two samples do not establish a stable average or percentile.

First Vulkan inference prepares driver shader/pipeline caches. That cost is real
user-visible latency and must not be excluded from onboarding expectations or
used to market only warm throughput. Model caches are not equivalent to a
resident inference worker: these repetitions use fresh processes, while the
graphics driver may reuse disk/cache preparation between them.

The recognized key clauses matched the golden semantic criteria in every
successful recorded repetition. Base Q5_1 provided commas in this excerpt;
tiny.en provided the same content with fewer commas. One familiar public speech
is a smoke test, not an accuracy or accent benchmark. No cloud enhancement was
used in these audio measurements.

## Contention, resource priority and recovery

Workers run below normal priority and avoid taking every physical core.
When native/Rust compilation saturated the machine, tiny CPU totals measured
9.713 / 38.211 / 20.145 seconds. This substantial variability shows the cost of
preserving desktop/input availability under load. Do not combine those samples
with the initial lower-contention median or promise fixed latency on an arbitrary
laptop. Recording and the keyboard helper run separately from native inference.

The real-model golden suite passed semantic recognition, a genuine zero-device
GPU probe followed by CPU decoding, and cancellation/child termination. A run
while Vulkan shader compilation saturated the workstation took 99.18 seconds
for the three tests. Normal HTTP/model tests separately validate range resume,
range ignored by the server, dishonest range rejection, truncated body recovery,
corrupt partial deletion, path checks, process locks and cancellation.

No inference worker remains resident between operations. Available/total RAM,
DXGI dedicated/shared GPU memory, model-load and inference times are reported by
the native services. A GPU failure retains time for the separate CPU recovery
attempt; the GPU inference attempt gets at most half the remaining deadline.

## Dictionary and deterministic quality

The release `eagle-quality` harness completed its checked-in deterministic cases
for replacements, Unicode/emoji, meaningful repeated words, numbers, URLs,
emails, explicit corrections, code preservation, malicious source instructions,
blank/refusal results and unwanted Markdown. It compares local output and
enhancement validation against fixture expectations; it does not claim a cloud
model generated the supplied enhanced examples.

The measured release result for a 10,000-entry vocabulary index was:

| Operation | Measurement | Scope |
| --- | ---: | --- |
| Build dictionary index | 80.19 ms | 10,000 synthetic vocabulary terms |
| Mean candidate retrieval | 0.001638 ms (1.638 microseconds) | 1,000 repeated exact-term queries |

Retrieval is an exact synthetic query (`ProjectTerm3842`), not a worst-case fuzzy
search, full dictionary correction accuracy result, or multilingual benchmark.
These values were recorded from the root release harness; run the command below
to obtain numbers on another machine or after changes.

## Reproduction

```powershell
cargo run --locked --release -p eagle-core --bin eagle-quality
.\scripts\asr-benchmark.ps1 -ModelId whisper-tiny-en
.\scripts\asr-benchmark.ps1 -SkipBuild -ModelId whisper-base-en-q5
.\scripts\asr-benchmark.ps1 -SkipBuild -ExecutionPreference gpu -CheckVulkan
```

The ASR script checks the fixture, installs/verifies the selected model outside
the repository, reports hardware and each iteration, then runs the semantic
golden suite. Optional GPU workers must first be built/staged with
`scripts/prepare.ps1 -Release -WithVulkan`; see [ASR](asr.md) for LLVM/SDK and
short target-path requirements. Report both first-use and repeated measurements
and whether other work is saturating the computer.

Application operations also emit local spans for audio finalization, ASR,
dictionary processing, cloud work and insertion. No complete hotkey-to-Notepad
latency distribution, live ChatGPT first-token/completion latency, ordinary
laptop accuracy corpus or fleet p95 measurements have yet been recorded. Those
need an interactive target, eligible account and representative hardware.

## Production worker verification

Both production workers were built with `--release --locked` and staged for the
installer. The CPU executable is 2,761,728 bytes. The separate Vulkan executable
is 56,921,600 bytes, including generated native shaders, and links the system
driver loader; the baseline CPU worker has no Vulkan dependency. The Vulkan
release used the bundled Ninja generator after nested MSBuild projects failed.

A real release Vulkan worker decoded the same fixture with both expected clauses
in 1,238 ms total (334 ms model load, 160 ms native inference; warm driver cache).
The three golden tests also passed against the release CPU worker under build
contention (120.99 seconds overall). The test harness itself ran in Cargo's test
profile; inference ran in the actual packaged release executable through pipes.
The separate physical-Vulkan golden gate passed against the production worker in
1.04 seconds, asserting the actual Vulkan route and both semantic speech clauses.
This gate is optional on CPU-only/CI machines and explicit with `-CheckVulkan`.
