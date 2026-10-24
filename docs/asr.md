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

The native process runs with `CREATE_NO_WINDOW` and below-normal priority. By
default, it uses at most six physical cores and reserves one physical core when
more than one exists. Advanced thread counts are limited to 1–16. It exits after
one operation, giving up model memory while idle. This adds the measured model
load/process startup cost to each operation and makes cancellation decisive.
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

Quality steps down to Balanced below 2 GiB available RAM; Balanced steps down to
Fast below 1 GiB. These are selection estimates, not an OS memory reservation.
Automatic language detection selects multilingual weights. Language `en` permits
English-only weights. Vocabulary candidates enter Whisper's initial prompt;
recognition remains probabilistic, and deterministic dictionary correction is a
separate core responsibility.

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

Distribute the CPU worker and optional renamed Vulkan worker beside the app.
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
are not p95 fleet measurements. Under concurrent native/Rust builds, below-normal
CPU worker latency ranged from 9.7 to 38.2 seconds; preserving desktop input
availability has a measurable cost under contention. The three real golden tests
(semantic recognition, honest zero-device probe CPU fallback, native cancellation)
all passed even during the shader build. The 12 normal tests cover hardware/model
selection, checksum rejection, path boundaries, locking, cancellation, resumable
HTTP ranges, an ignored range, a dishonest range, and truncated response recovery.
