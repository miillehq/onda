# Local speech recognition and model assets

Onda uses `whisper-rs 0.16.0`, its `whisper-rs-sys 0.15.0` binding, and the
binding's vendored `whisper.cpp 1.8.3`. The desktop shell depends on the process
client in `onda-asr` with no native feature enabled. Native inference exists
only in the separately packaged `onda-asr-worker.exe`. No server, account,
API key, Python runtime or GPU is needed for ordinary dictation.

## Worker boundary

`AsrEngine::transcribe` takes mono 16,000 Hz normalized finite `f32` PCM,
`TranscriptionOptions`, and a `CancellationToken`. The operation checks audio,
options and worker/model existence before admitting one inference. Additional
operations return `Busy` instead of silently accumulating work.

Audio crosses inherited stdin pipes as a bounded JSON header plus little endian
PCM. It never crosses command-line arguments or temporary audio files. A worker
returns a bounded JSON transcript and timing metrics; native stderr is discarded
by the client so upstream logging cannot reveal dictation in application logs.
The benchmark deliberately prints its public fixture transcript.

The native process runs with `CREATE_NO_WINDOW` at normal priority and opts out of
Windows power throttling (`ProcessPowerThrottling`, execution speed). At below-normal
priority, Windows parked the windowless worker on efficiency cores after the machine
idled while the user spoke: on an i5-14400F, warm tiny inference ranged 68–1197 ms
and small Q5 397–1754 ms, against 69–149 ms and 378–436 ms at normal priority. The
shortcut helper runs above normal, so the keyboard hook still comes first. By
default the worker uses every physical core up to six; reserving a core left two-
and four-core laptops with one or three threads. Advanced thread counts are limited
to 1–16.

## Speed

Measured on an i5-14400F (6 performance and 4 efficiency cores) with a discrete
GPU, Windows 11, the public JFK fixture cut to 3 s and its full 11 s:

- **Optimized native build.** The `cmake` crate replaces CMake's per-configuration
  flags under the Visual Studio generator and dropped `/O2`, so whisper.cpp in every
  CPU worker compiled with optimization disabled (`/Od`); the Ninja-built Vulkan
  worker kept CMake's defaults. `.cargo/config.toml` now defines
  `CMAKE_{C,CXX}_FLAGS_RELEASE` with MSVC's Release defaults, which `whisper-rs-sys`
  forwards. CPU inference fell from 445 to 89 ms (tiny, 3 s), 1625 to 221 ms (tiny,
  11 s), 6968 to 924 ms (small Q5, 3 s) and 17069 to 1354 ms (small Q5, 11 s), with the
  same words. `whisper-rs-sys` does not rerun on these variables, so existing build
  directories need `cargo clean -p whisper-rs-sys`.
- **Warm start.** The worker protocol sends setup and audio separately
  (`WorkerSetup`, then `WorkerAudio` and PCM). Recording start calls
  `AsrEngine::prepare`, so model verification, worker launch, model load and GPU
  initialization overlap with speech; `PreparedTranscription::finish` sends the
  audio. A cancelled or failed recording drops it, which kills the worker, so nothing
  stays loaded between dictations. With an idle gap equal to the clip, latency after
  speaking fell from 1640 to 85 ms (tiny, 3 s, Auto on CPU), 893 to 211 ms (small Q5,
  3 s, Auto on GPU) and 1155 to 383 ms (small Q5, 11 s, Auto on GPU).
- **Model verification once per session.** The SHA-256 of the model (75–190 MB) is
  checked the first time each session and again only when its size or modification
  time changes; it had run before every dictation (86–175 ms here, more on slower
  disks).
- **GPU probe once per session.** The Vulkan device probe result is remembered; a
  failed Vulkan run marks it unavailable for the session.
- **Auto picks the faster device.** With the optimized CPU build, a GPU worker's
  device setup outweighs its inference for tiny and base models, so Auto uses the
  CPU below 150 MB of model and the GPU for small (Quality). GPU preferred still
  forces the accelerator.
- **Flash attention stays off.** It was slower on the CPU (tiny, 3 s: 445 to
  1007 ms before the optimization fix) and saved under 20 ms on the GPU, while
  shifting punctuation (small Q5 dropped the commas in "And so, my fellow
  Americans,"). whisper.cpp 1.8 enables it by default; `whisper-rs` 0.16 does not.

Whisper's encoder always covers a 30-second window (1500 frames of 20 ms). The

Whisper's encoder always covers a 30-second window (1500 frames of 20 ms). The
worker sets `audio_ctx` to the clip's own frames plus a 128-frame margin, and
keeps the full window from about 27 seconds up. On the public JFK fixture with the
tiny model this cut a 3-second clip from about 3.2 s to 0.45 s of inference with
the same words, and the 11-second fixture from 3.3 s to 1.4 s (one comma differed),
measured before the build fix above. A worker serves one recording and exits,
giving up model memory while idle; the warm start hides its startup and keeps
cancellation decisive.
Cancellation and the 180-second default timeout kill and reap the child. Native
crashes produce structured failure without crashing the shell or keyboard hook.
Each Windows child is immediately assigned to a private, unnamed, non-inherited
Job Object before any PCM is sent. Its `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` handle
stays owned by the inference/probe future. Windows closes that handle when the
parent crashes or exits through an updater's immediate process termination,
killing associated workers even when Tokio destructors cannot run. Assignment
failure kills the just-spawned child and fails the operation. Windows 10/11
support nested jobs, including children launched by an existing runner job.
This lifecycle follows [Microsoft's Job Object
semantics](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).
The recording limit is ten minutes; the worker rejects truncated/extra PCM,
NaN/Inf, amplitudes outside [-1, 1], malformed headers and embedded NUL prompts.
Silence with RMS below 0.0001 returns empty text, preventing obvious hallucination.

This is process isolation for availability, **not a Windows security sandbox**.
The worker runs as the same user. Only catalog-verified model paths should be
passed from application commands; external/custom model files are outside the
frontend catalog contract. The worker excludes the current working directory
from subsequent DLL loads. Optional accelerator DLL imports are contained in the
accelerator executable, never in the shell or baseline CPU worker.

## CPU and GPU preferences

The required worker uses generic x64 code. Repository Cargo environment forces
`GGML_NATIVE`, AVX, AVX2, AVX512, FMA, F16C, BMI2 and SSE4.2 off. SSE2 is part of
the x64 architectural baseline. The CMake cache was inspected to verify those
flags. A build host's instruction set therefore cannot leak into the distributed
baseline. Optional accelerator features must be built in a **different target
directory**, preserving a CPU worker that has no Vulkan loader dependency.

`prepare.ps1` also builds the same worker with AVX, AVX2, FMA, F16C, SSE4.2 and
BMI2 on (`--config env.GGML_*.value='ON'`, target `target/avx2`) and ships it as
`onda-asr-avx2-worker.exe`. The engine uses it when the CPU reports AVX2, FMA,
F16C and BMI2 (Haswell-class, 2013+), and retries the baseline worker if it
fails. On the JFK fixture with the tiny model and six threads, the baseline took
about 19 s of inference and the AVX2 build about 3.2 s, before the `audio_ctx`
change above.

Hardware diagnostics enumerate logical/physical CPUs, available/total RAM,
runtime SIMD evidence, and DXGI adapters with vendor IDs, dedicated/shared memory
and software-adapter flags. CUDA, DirectML and Vulkan system DLL presence is
reported explicitly as presence, not as proof an inference backend is usable.

| Preference | Behavior |
| --- | --- |
| CPU only | Never probe or execute acceleration. |
| Auto | Probe a packaged Vulkan worker when a hardware adapter has at least 512 MiB dedicated memory and the system Vulkan loader exists. |
| GPU preferred | Also permit integrated adapters; successful isolated device enumeration is still required. |

Missing GPU worker, missing loader, no compatible Vulkan device, probe failure,
driver crash or inference failure retries the separate CPU worker. The worker
discards native log contents but detects pinned initialization failure markers,
preventing whisper.cpp's internal CPU recovery from being misreported as Vulkan.
GPU preferred
with unavailable acceleration returns `acceleratorFallback: true`. Cancellation
never causes a CPU retry. GPU and CPU attempts share the overall deadline, with
GPU inference limited to half the remaining time so a hung driver leaves a CPU
recovery budget.
Integrated Intel/AMD graphics therefore always retain CPU dictation. Vulkan is
the optional cross-vendor backend; CUDA and DirectML were researched and are not
distributed inference paths in this version. See [ASR references](asr-references.md).

## Profiles and languages

| Profile | English | Automatic/other language | Estimated memory |
| --- | --- | --- | --- |
| Fast (default) | tiny.en F16 | tiny F16 | 400 MiB |
| Balanced | base.en Q5_1 | base Q5_1 | 600 MiB |
| Quality | small.en Q5_1 | small Q5_1 | 1,200 MiB |
| Advanced | Explicit catalog model ID | Explicit catalog model ID | Catalog estimate |

Quality steps down to Balanced below 6 GiB of installed RAM; Balanced steps down
to Fast below 3 GiB (`manifest.rs`). Installed rather than free RAM is used so the
choice does not change between launches. These are selection estimates, not an OS
memory reservation. On Linux, installed RAM comes from `/proc/meminfo`; before that
was read, Linux reported 0 bytes and every profile stepped down to Fast.
Automatic language detection selects multilingual weights. Language `en` permits
English-only weights. Vocabulary candidates enter Whisper's initial prompt;
recognition remains probabilistic, and deterministic dictionary correction is a
separate core responsibility.

### Which languages each model supports

The spoken-language setting becomes whisper.cpp's language token (`set_language`), or no token
for automatic detection. Onda never translates (`translate: false`) and reports no detected
language.

| Catalog model | Multilingual | Language tokens | Cantonese (`yue`) |
| --- | --- | --- | --- |
| whisper-tiny-en, whisper-base-en-q5, whisper-small-en-q5 | No | none; the setting is ignored and output is English | No |
| whisper-tiny, whisper-base-q5, whisper-small-q5 | Yes | 99 (`onda_asr::WHISPER_LANGUAGES`) | No |

- **Evidence.** The ggml files at revision `5359861c` have 51,864 (English-only) and 51,865
  (multilingual) vocabulary entries; whisper.cpp derives 99 languages from 51,865. Large-v3 and
  its turbo variant have 51,866 entries, the 100th language being `yue`, added to OpenAI's
  tokenizer in the large-v3 commit (`openai/whisper@c5d4256`, `tokenizer.py`). Onda offers no
  large-v3 model.
- **Why `yue` is refused.** whisper.cpp 1.8.3 (vendored by whisper-rs-sys 0.15.0) computes a
  language token as `sot + 1 + id` without checking it against the model. For a 99-language model,
  `yue` (id 99) becomes `<|translate|>`, a prompt the model never saw in training, with no error.
  Settings, the speech engine and the worker therefore refuse codes outside the 99.
- **"Chinese".** Whisper has one `zh` token for all varieties of Chinese. Its training data and the
  paper's evaluation are mostly Mandarin (FLEURS and Common Voice `zh-CN`), and output may be
  Simplified or Traditional, sometimes mixed (OpenAI maintainers in openai/whisper discussions 277
  and 987). The interface labels it "Chinese (Mandarin)" and explains this. For the catalog's
  sizes the paper reports Chinese character error rates of 40.5 (tiny), 34.1 (base) and 20.8
  (small) on FLEURS. Those are unquantized models, not Onda's Q5_1 files, so they are only a rough
  guide.
- **Cantonese.** It cannot be selected. Under `zh` or automatic detection, Cantonese speech is
  decoded through the Chinese token. Community reports (ggml-org/whisper.cpp issue 2319) say
  automatic detection labels it Chinese. How accurate the output is has not been measured;
  Onda makes no claim either way.
- **English-only models.** A custom selection can pair an English-only model with another
  language. Settings › Speech and language then warns that the setting is ignored.

## Model manager

`manifest.rs` is the immutable model trust root. Six files are pinned to Hugging
Face repository revision `5359861c739e955e79d9a303bcbc70fb988958b1`. Exact byte
counts and SHA-256 values came from its `?blobs=true` API metadata on 3 October
2026, then real downloaded tiny.en/base.en files were independently hashed.
URLs are HTTPS; redirects are restricted to HTTPS Hugging Face/hf.co hostnames
with an eight-redirect limit. Renderer input chooses a catalog ID, never a URL.

`ModelManager` canonicalizes its configured directory, derives filenames only
from trusted IDs/revisions, rejects symlink and directory targets, and uses an
OS file lock to prevent cross-process download/uninstall races. It checks free
space for remaining bytes plus a 64 MiB reserve. Downloads keep `.part` files,
request byte ranges, validate `Content-Range` and exact length, and restart from
zero when the server returns 200 instead of honoring resume. Bounded progress is
emitted at approximately ten events per second plus stage changes. Cancellation
retains the resumable prefix. Interrupted HTTP bodies cannot become installed.

Only a full SHA-256/size match reaches fsync and an atomic rename on the same
volume. Checksum failure deletes the corrupt partial; the next download can
recover. Existing corrupted installations are replaced by a fresh verified
download. `verified_model_path` checks installed bytes before native use;
`installed` is lightweight size/catalog enumeration, not checksum certification.
Do not infer trust solely from `installed`.

Files include their full model revision; application upgrades preserve them.
Older revisions are left on disk for explicit recovery/removal rather than
silently deleted. Manifest schema version starts at 1. `migrate_to` hashes and
copies current assets in bounded chunks, fsyncs, rechecks hashes and atomically
installs at a new location. It preserves the old directory and returns the new
manager only after success; save the new setting then. A cancelled migration
does not break the existing location. Uninstall is explicit and lock-protected.
No model weights or personal recordings belong in Git.

## Building and verifying

Install Rust/MSVC x64, Windows SDK, CMake, and LLVM with `libclang.dll`. Set
`LIBCLANG_PATH` if LLVM is outside its standard install location. Windows must
generate its own FFI bindings: do **not** set `WHISPER_DONT_GENERATE_BINDINGS`,
because the crate's bundled Linux bindings have incompatible Windows ABI layout.
The optional Vulkan build also needs the official [LunarG SDK](https://vulkan.lunarg.com/sdk/home).

```powershell
cargo test -p onda-asr
cargo build -p onda-asr --release --features native-asr --bins
.\scripts\asr-benchmark.ps1 -SkipBuild
```

The benchmark installs an exact-hash model outside the repository, verifies the
public speech fixture, emits real timings, and runs semantic golden tests.
After setting `ONDA_ASR_WORKER_DIR` to a built worker directory, the controlled
Windows lifecycle test keeps the real worker's stdin open, closes its private
job, and confirms prompt process termination:

```powershell
cargo test --locked -p onda-asr --lib closing_job_kills_owned_worker_even_with_stdin_open -- --ignored
```

For acceleration, use a short build directory because MSBuild's nested Vulkan
shader project can exceed MAX_PATH even when normal Rust compilation succeeds:

```powershell
. .\scripts\toolchain.ps1
. .\scripts\vulkan-toolchain.ps1
cargo build --locked -p onda-asr --release --features vulkan --bin onda-asr-worker --target-dir C:\evn
Copy-Item -LiteralPath C:\evn\release\onda-asr-worker.exe -Destination target\release\onda-asr-vulkan-worker.exe
.\scripts\asr-benchmark.ps1 -SkipBuild -ExecutionPreference gpu -CheckVulkan
```

Distribute the CPU workers (baseline and AVX2) and optional renamed Vulkan worker beside the app.
Do not replace the CPU worker with the GPU build. End users need an appropriate
graphics driver, not the SDK. Test CPU-only launch on a machine without Vulkan.
The tested production build uses Ninja, configured by `vulkan-toolchain.ps1`.
That helper imports a verified x64 MSVC developer environment without exposing
its contents. Nested MSBuild shader projects failed both normal workspace paths
and a short release target; Ninja passed with the same SDK/source. Keep CPU and
GPU targets separate and use `ONDA_VULKAN_TARGET_DIR` for the release helper's
short absolute target. Restore `CMAKE_GENERATOR` after optional builds when
reusing a shell whose existing CPU cache was generated with MSBuild.

The native workers' VC++ runtime requirements must be handled by release
packaging. `dumpbin` confirms imports of MSVCP140, VCRUNTIME140 and
VCRUNTIME140_1. Permitted app-local redistribution avoids requiring administrator
installation of a global VC++ redistributable. Follow the Visual Studio licensed
redistribution list and maintain these DLLs through app releases; Microsoft cannot
service local copies through Windows Update. See [Microsoft's redistribution
requirements](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-170)
and [deployment options](https://learn.microsoft.com/en-us/cpp/windows/deployment-in-visual-cpp?view=msvc-170).
Windows ARM64 is architecturally possible but no ARM64 worker has been
built or certified; x64 is the required release architecture.

The checked fixture is a narrow recognition smoke test. Real-device acceptance
still needs a broader accent/language/noise corpus and older CPU, AMD/Intel GPU,
RDP and constrained-memory machines. Measured values belong in
[benchmarks](benchmarks.md), not marketing claims of instant recognition.

## Recorded native proof

On 3 October 2026 the real 11-second JFK fixture was decoded on an Intel
Core i5-14400F (10 physical/16 logical cores), 16 GiB RAM, NVIDIA RTX 5060
(approximately 8 GiB dedicated memory), with six ASR CPU threads and a generic
CPU build. Each repetition launched a fresh worker. Both key speech clauses
were recognized in every successful recorded run.

| Model/backend | Total latency ms | Native inference ms | Observation |
| --- | --- | --- | --- |
| tiny.en CPU | 3,079 / 2,954 / 2,957 | 2,761 / 2,712 / 2,728 | Initial low-contention sample. |
| base.en Q5 CPU | 8,363 / 8,761 / 8,046 | 8,133 / 8,551 / 7,839 | Better punctuation; higher CPU cost. |
| tiny.en Vulkan | 9,750 then 1,019 / 1,009 | 8,782 then 105 / 101 | First shader preparation, then warm driver cache. |
| base.en Q5 Vulkan | 25,224 then 2,106 / 1,543 | 13,796 then 224 / 196 | First-use preparation and concurrent build contention; subsequent cache reuse. |

These small samples demonstrate real recognition and backend execution. They
are not p95 fleet measurements. Under concurrent native/Rust builds, the earlier
below-normal, unoptimized CPU worker's latency ranged from 9.7 to 38.2 seconds; preserving desktop input
availability has a measurable cost under contention. The three real golden tests
(semantic recognition, honest zero-device probe CPU fallback, native cancellation)
all passed even during the shader build. The 12 normal tests cover hardware/model
selection, checksum rejection, path boundaries, locking, cancellation, resumable
HTTP ranges, an ignored range, a dishonest range, and truncated response recovery.
