# Local models for AI polish

The **On this computer** provider sends polish to a model server running on the same machine:
Ollama, LM Studio or llama.cpp's `llama-server`. Onda checks that the address is on this computer
and never falls back to a cloud provider.

## Why a local server rather than a model inside Onda

Onda already embeds whisper.cpp in isolated worker processes, and embedding llama.cpp the same way
was considered. A separate server was chosen because:

- New model architectures land in llama.cpp almost weekly (Qwen3.5's hybrid attention first worked
  in build b7990, with fixes into b8209). Shipping our own copy would mean rebuilding and
  re-releasing Onda for each one; Ollama, LM Studio and llama.cpp already do this.
- GPU support differs per machine (CUDA, ROCm, Vulkan, Metal). Those servers ship builds for each,
  while Onda ships CPU and one Vulkan whisper worker. A second inference stack would multiply that.
- Model files are 0.5–10 GB. Users who already run a local server keep one copy for every app.
- It reuses the OpenAI-compatible adapter, so the code path is small and already tested.

The cost is one extra install. If that proves too much, an embedded worker can follow the same
design as the whisper worker (pinned model manifest, verified download, isolated process); the
provider interface would not change.

## Qwen3.5 small models (verified 2026-10-29)

- `Qwen/Qwen3.5-0.8B` exists on Hugging Face, released 2026-03-02 with 2B, 4B and 9B siblings,
  under the Apache-2.0 licence. It is multimodal (a vision encoder) with hybrid Gated DeltaNet and
  attention layers and a 262,144-token context.
  Sources: <https://huggingface.co/Qwen/Qwen3.5-0.8B>, <https://github.com/QwenLM/Qwen3.5>.
- Its card says it does not think by default, but **llama-server (`--reasoning auto`) and Ollama's
  qwen3.5 renderer turn thinking on unless the request turns it off**. Onda sends both
  `chat_template_kwargs: {"enable_thinking": false}` (llama.cpp) and `reasoning_effort: "none"`
  (Ollama, llama.cpp), and strips any leftover `<think>` block.
- Qwen publishes no GGUF files. Community GGUFs: `unsloth/Qwen3.5-0.8B-GGUF` (Q4_K_M 532.5 MB,
  SHA-256 `bd258782…dc517`), `ggml-org`, `bartowski` and `lmstudio-community`. The vision projector
  (about 205 MB) is not needed for text.
- Runtimes: llama.cpp b7990 or later (current stable b11960 used here), Ollama 0.30.0 or later
  (`qwen3.5:0.8b`, about 1.3 GB with its projector and draft layer), LM Studio 0.4.6 or later via the
  lmstudio-community repo. llama.cpp's default port changed from 8080 to 9931 in b11521; Onda's
  presets use 9931 and the field hint mentions 8080.
- LM Studio's handling of `enable_thinking` over `/v1` is unverified; its bug tracker reports it is
  ignored. If LM Studio rejects either switch, Onda retries once without them on the same server.

## Measured on this VPS (CPU only)

Shared 4-vCPU Intel Haswell VPS (AVX2, no AVX-512), about 5 GB free memory, llama.cpp b11960
Ubuntu x64 CPU build (release asset SHA-256 `31cc3a6b…1d97a`), Qwen3.5-0.8B Q4_K_M, `-c 4096 -t 3`:

| Measure | Result |
| --- | --- |
| Model load | 4.4 s |
| `llama-bench` prompt processing (pp256) | 92 tokens/s |
| `llama-bench` generation (tg64) | 13.7 tokens/s |
| Server resident memory | about 960 MB |
| Onda's live test (one 20-word sentence through the Local adapter) | 2.3–4.0 s, 3 runs, no `<think>` output |

The 0.8B model is light: in the live test it removed "uh" but kept "um". Onda's preservation check
still guards every answer, and a rejected answer keeps the original text. Prefer 2B or 4B where
memory allows.

## Guidance shown in the app

| Computer | Suggested model | Memory | Expectation |
| --- | --- | --- | --- |
| CPU only, 8 GB | Qwen3.5 0.8B Q4_K_M | about 1 GB | 2–4 s per sentence on 3 older cores (measured) |
| Recent laptop, 16 GB | Qwen3.5 2B Q4_K_M (1.28 GB file) | about 2 GB | Better wording; Vulkan builds use integrated graphics |
| Dedicated GPU, 8 GB+ | Qwen3.5 4B Q4_K_M (2.74 GB file) | about 3–4 GB | Fast on the GPU; its template thinks by default, which Onda turns off |

Only the CPU row was measured. The GPU rows are estimates from file sizes and are untested.

## Reproduce the CPU check

```bash
curl -LO https://github.com/ggml-org/llama.cpp/releases/download/b11960/llama-b11960-bin-ubuntu-x64.tar.gz
echo "31cc3a6bc638ac6c48dc4b1b74888f1eaadf3d14227c1e5f67daf9a49541d97a  llama-b11960-bin-ubuntu-x64.tar.gz" | sha256sum -c
curl -LO https://huggingface.co/unsloth/Qwen3.5-0.8B-GGUF/resolve/main/Qwen3.5-0.8B-Q4_K_M.gguf
echo "bd258782e35f7f458f8aced1adc053e6e92e89bc735ba3be89d38a06121dc517  Qwen3.5-0.8B-Q4_K_M.gguf" | sha256sum -c
tar -xzf llama-b11960-bin-ubuntu-x64.tar.gz
llama-b11960/llama-server -m Qwen3.5-0.8B-Q4_K_M.gguf -c 4096 -t 3 --host 127.0.0.1 --port 9931 --alias qwen3.5-0.8b &
ONDA_LOCAL_LLM_URL=http://127.0.0.1:9931/v1 ONDA_LOCAL_LLM_MODEL=qwen3.5-0.8b \
  cargo test -p onda-cloud --lib live_local_server_polishes_text -- --ignored --nocapture
```

## GPU test procedure (to run on a machine with a GPU)

Nothing here was run on a GPU. Record the results in testing.md.

1. **llama.cpp, Windows Vulkan:** download `llama-b11960-bin-win-vulkan-x64.zip` from the same
   release and check its SHA-256 against the release page. Run
   `llama-server -m Qwen3.5-0.8B-Q4_K_M.gguf -c 4096 -ngl 99 --host 127.0.0.1 --port 9931 --alias qwen3.5-0.8b`.
   Expect the log to name your GPU and to offload all layers.
2. Run `llama-bench -m Qwen3.5-0.8B-Q4_K_M.gguf -ngl 99 -p 256 -n 64 -r 3`, and again with `-ngl 0`.
   Record both and the driver version.
3. **Through Onda:** Settings › AI polish › On this computer › llama.cpp, then Check connection. It
   should list `qwen3.5-0.8b`. Turn on Polish my dictation and dictate the same sentence 5 times.
   Note the time from release to text, and that `<think>` never appears.
4. **Ollama:** `ollama pull qwen3.5:0.8b` (and `qwen3.5:4b` on a GPU with 8 GB+), choose the Ollama
   preset, then repeat step 3. `ollama ps` should show the GPU percentage.
5. **Failure paths:** stop the server and dictate. Expect "Nothing answered at 127.0.0.1:…", the
   local text typed, and no request to any other host (check with Resource Monitor's Network tab
   or `netstat -b`). Then enter `http://192.168.x.x:11434/v1` as the address. Expect it to be
   refused before saving.
6. Repeat step 3 with `Qwen3.5-2B` and `4B` and note which quantization fits your VRAM.
