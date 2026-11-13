# Local speech recognition and model assets

Onda uses `whisper-rs 0.16.0`, its `whisper-rs-sys 0.15.0` binding, and the binding's vendored
`whisper.cpp 1.8.3`. The desktop shell depends on the process client in `onda-asr` with no
native feature enabled. Native inference exists only in the separately packaged workers:
`onda-asr-worker` (baseline CPU), `onda-asr-avx2-worker` and, in optional GPU builds,
`onda-asr-vulkan-worker`. On macOS the workers are built with Metal. No server, account, API key,
Python runtime or GPU is needed for ordinary dictation.

## Worker boundary

`AsrEngine::transcribe` takes mono 16,000 Hz normalized finite `f32` PCM,
`TranscriptionOptions`, and a `CancellationToken`. The operation checks audio, options and
worker/model existence before admitting one inference. Additional operations return `Busy`
instead of silently accumulating work.

The protocol sends setup and audio separately (`WorkerSetup`, then `WorkerAudio` and PCM).
Recording start calls `AsrEngine::prepare`, so model verification, worker launch, model load and
GPU initialization overlap with speech; `PreparedTranscription::finish` sends the audio. A
cancelled or failed recording drops it, which kills the worker, so nothing stays loaded between
dictations. Audio crosses inherited stdin pipes as a bounded JSON header plus little-endian PCM,
never command-line arguments or temporary files. A worker returns a bounded JSON transcript and
timing metrics; native stderr is discarded so upstream logging cannot reveal dictation in
application logs. The benchmark deliberately prints its public fixture transcript.

On Windows the worker runs with `CREATE_NO_WINDOW` at normal priority and opts out of power
throttling: at below-normal priority, Windows parked the windowless worker on efficiency cores
and inference times varied several-fold ([benchmarks.md](benchmarks.md)). The shortcut helper
runs above normal, so the keyboard hook still comes first. By default the worker uses every
physical core up to six; advanced thread counts are limited to 1–16.

The model's SHA-256 is checked the first time each session and again only when its size or
modification time changes. The Vulkan probe result is remembered for the session, and a failed
Vulkan run marks it unavailable. Flash attention stays off: it was slower on the CPU and shifted
punctuation on the GPU. Whisper's encoder covers a 30-second window; the worker sets `audio_ctx`
to the clip's own frames plus a 128-frame margin, and keeps the full window from about
27 seconds up, which cuts inference for short clips several times over.

Cancellation and the 180-second default timeout kill and reap the child. Native crashes produce
structured failure without crashing the shell or keyboard hook. On Windows each child is assigned
at once to a private Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, owned by the
inference/probe future, so workers die with the parent even when it is killed by an updater;
assignment failure kills the child and fails the operation. The recording limit is ten minutes;
the worker rejects truncated/extra PCM, NaN/Inf, amplitudes outside [-1, 1], malformed headers
and embedded NUL prompts. Silence with RMS below 0.0001 returns empty text, preventing obvious
hallucination.

This is process isolation for availability, **not a security sandbox**. The worker runs as the
same user. Only catalog-verified model paths are passed from application commands. The worker
excludes the current working directory from subsequent DLL loads. Optional accelerator imports
are contained in the accelerator executable, never in the shell or baseline CPU worker.

## CPU and GPU preferences

The baseline x86-64 worker uses generic code: the repository's Cargo environment turns
`GGML_NATIVE`, AVX, AVX2, AVX512, FMA, F16C, BMI2 and SSE4.2 off, so a build host's instruction
set cannot leak into the package. `.cargo/config.toml` also sets `CMAKE_{C,CXX}_FLAGS_RELEASE` to
MSVC's Release defaults, because the `cmake` crate otherwise built whisper.cpp without
optimization under the Visual Studio generator; after changing those variables, run
`cargo clean -p whisper-rs-sys`. `prepare.ps1` and `prepare.sh` also build the worker with AVX,
AVX2, FMA, F16C, SSE4.2 and BMI2 (`target/avx2`) as `onda-asr-avx2-worker`. The engine uses it
when the CPU reports AVX2, FMA, F16C and BMI2 (Haswell-class, 2013+), and retries the baseline
worker if it fails. ARM has no AVX2 build: ARM packages carry the baseline worker under both
names, and the engine never selects the AVX2 name there.

On Windows, hardware diagnostics enumerate logical/physical CPUs, available/total RAM, runtime
SIMD evidence, and DXGI adapters with vendor IDs, dedicated/shared memory and software-adapter
flags. CUDA, DirectML and Vulkan system DLL presence is reported as presence, not as proof an
inference backend is usable.

| Preference | Behavior |
| --- | --- |
| CPU only | Never probe or execute acceleration. |
| Auto | Probe a packaged Vulkan worker when a hardware adapter has at least 512 MiB dedicated memory and the system Vulkan loader exists, and use it for models of 150 MiB or more (Quality); smaller models run faster on the CPU once the GPU's setup is counted. |
| GPU preferred | Also permit integrated adapters, for every model; successful isolated device enumeration is still required. |

Missing GPU worker, missing loader, no compatible Vulkan device, probe failure, driver crash or
inference failure retries the separate CPU worker. The worker discards native log contents but
detects pinned initialization failure markers, so whisper.cpp's internal CPU recovery is not
misreported as Vulkan. GPU preferred with unavailable acceleration returns
`acceleratorFallback: true`. Cancellation never causes a CPU retry. GPU and CPU attempts share
the overall deadline, with GPU inference limited to half the remaining time so a hung driver
leaves a CPU recovery budget. CUDA and DirectML were researched and are not used
([references.md](references.md)).

Linux does not enumerate GPU adapters, so the engine does not try a Vulkan worker there today,
even with GPU preferred; dictation runs on the CPU workers. On macOS the worker offers Metal to
whisper.cpp, which uses the CPU when Metal is unavailable.

## Profiles and languages

| Profile | English | Automatic/other language | Estimated memory |
| --- | --- | --- | --- |
| Fast (default) | tiny.en F16 | tiny F16 | 400 MiB |
| Balanced | base.en Q5_1 | base Q5_1 | 600 MiB |
| Quality | small.en Q5_1 | small Q5_1 | 1,200 MiB |
| Custom (All models) | Explicit catalog model ID | Explicit catalog model ID | Catalog estimate |

Quality steps down to Balanced below 6 GiB of installed RAM; Balanced steps down to Fast below
3 GiB (`manifest.rs`). Installed rather than free RAM is used so the choice does not change
between launches. Windows reads it from the system and Linux from `/proc/meminfo`; macOS does not
read it yet, so there Balanced and Quality resolve to the Fast models unless a model is chosen
under All models. Automatic language detection selects multilingual weights. Language `en`
permits English-only weights. Vocabulary candidates enter Whisper's initial prompt; recognition
remains probabilistic, and deterministic dictionary correction is a separate core
responsibility.

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
- **Why `yue` is refused.** whisper.cpp 1.8.3 computes a language token as `sot + 1 + id` without
  checking it against the model. For a 99-language model, `yue` (id 99) becomes `<|translate|>`,
  a prompt the model never saw in training, with no error. Settings, the speech engine and the
  worker therefore refuse codes outside the 99.
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

`manifest.rs` is the immutable model trust root. Six files are pinned to Hugging Face repository
revision `5359861c739e955e79d9a303bcbc70fb988958b1`, with exact byte counts and SHA-256 values
from its `?blobs=true` API metadata, checked against independently hashed downloads. URLs are
HTTPS; redirects are restricted to HTTPS Hugging Face/hf.co hostnames with an eight-redirect
limit. Renderer input chooses a catalog ID, never a URL.

`ModelManager` canonicalizes its configured directory, derives filenames only from trusted
IDs/revisions, rejects symlink and directory targets, and uses an OS file lock to prevent
cross-process download/uninstall races. It checks free space for remaining bytes plus a 64 MiB
reserve. Downloads keep `.part` files, request byte ranges, validate `Content-Range` and exact
length, and restart from zero when the server returns 200 instead of honoring resume. Bounded
progress is emitted at approximately ten events per second plus stage changes. Cancellation
retains the resumable prefix. Interrupted HTTP bodies cannot become installed.

Only a full SHA-256/size match reaches fsync and an atomic rename on the same volume. Checksum
failure deletes the corrupt partial; the next download can recover. Existing corrupted
installations are replaced by a fresh verified download. `verified_model_path` checks installed
bytes before native use; `installed` is lightweight size/catalog enumeration, not checksum
certification. Do not infer trust solely from `installed`.

Files include their full model revision; application upgrades preserve them. Older revisions are
left on disk for explicit recovery/removal rather than silently deleted. `migrate_to` hashes and
copies current assets in bounded chunks, fsyncs, rechecks hashes and atomically installs at a new
location. It preserves the old directory and returns the new manager only after success; save the
new setting then. A cancelled migration does not break the existing location. Uninstall is
explicit and lock-protected. No model weights or personal recordings belong in Git.

## Building and verifying

`scripts/prepare.ps1` (Windows) or `scripts/prepare.sh` (Linux, macOS) builds the workers
([CONTRIBUTING.md](../CONTRIBUTING.md) lists the toolchains). On Windows, LLVM's `libclang.dll`
is needed; set `LIBCLANG_PATH` if LLVM is outside its standard location, and do **not** set
`WHISPER_DONT_GENERATE_BINDINGS`, because the crate's bundled Linux bindings have an
incompatible Windows ABI layout. Windows on ARM builds the worker with clang-cl and Ninja
(`scripts/arm64-toolchain.ps1`, which `prepare.ps1` loads on an ARM64 host).

```powershell
cargo test -p onda-asr
cargo build -p onda-asr --release --features native-asr --bins
.\scripts\asr-benchmark.ps1 -SkipBuild
```

The benchmark installs an exact-hash model outside the repository, verifies the public speech
fixture, emits real timings, and runs the golden tests ([testing.md](testing.md)). With
`ONDA_ASR_WORKER_DIR` set to a built worker directory, the Windows lifecycle test keeps the real
worker's stdin open, closes its private job, and confirms prompt termination:

```powershell
cargo test --locked -p onda-asr --lib closing_job_kills_owned_worker_even_with_stdin_open -- --ignored
```

The optional Vulkan worker needs the [LunarG SDK](https://vulkan.lunarg.com/sdk/home) to build
(end users need only a graphics driver) and a short target directory, because MSBuild's nested
shader project exceeds MAX_PATH; `vulkan-toolchain.ps1` configures Ninja instead:

```powershell
. .\scripts\toolchain.ps1
. .\scripts\vulkan-toolchain.ps1
cargo build --locked -p onda-asr --release --features vulkan --bin onda-asr-worker --target-dir C:\evn
Copy-Item -LiteralPath C:\evn\release\onda-asr-worker.exe -Destination target\release\onda-asr-vulkan-worker.exe
.\scripts\asr-benchmark.ps1 -SkipBuild -ExecutionPreference gpu -CheckVulkan
```

Keep CPU and GPU targets separate (`ONDA_VULKAN_TARGET_DIR` for the release helper), never
replace the CPU worker with the GPU build, and restore `CMAKE_GENERATOR` after optional builds
when reusing a shell whose CPU cache was generated with MSBuild.

The Windows workers import the VC++ runtime (MSVCP140, VCRUNTIME140, VCRUNTIME140_1). Onda ships
app-local copies so a per-user install needs no administrator, and must ship serviced copies with
its releases; see [third-party-licenses.md](third-party-licenses.md).

The JFK fixture is a narrow recognition smoke test. Accents, languages, noise, older CPUs, AMD
and Intel GPUs and constrained-memory machines are not covered by it. Measured values belong in
[benchmarks.md](benchmarks.md).
