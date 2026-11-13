# ADR 001: Tauri/Rust with isolated keyboard and inference processes

Status: accepted. Date: 2026-10-03.

The application needs Windows global hold/release shortcuts, low idle cost, real local ASR and a future frontend design session. Tauri keeps business logic in typed Rust and uses the installed WebView2. Electron offers no native input advantage that removes the need for Windows APIs and carries another bundled runtime. WinUI would demand a different UI/backend language boundary and divert work from the frontend ownership plan.

Choose Tauri v2 as a thin shell. Put ASR and the keyboard hook in different native processes. Dedicated hook threads alone do not isolate a native model crash or process-wide contention. The cost is two small extra executables, pipe framing, supervision and packaging. They are built and tested on Windows. Inference thread count and priority are limited; cancellation kills/reaps an inference worker. Optional Vulkan cannot prevent the CPU shell from launching.

Do not adopt WhimprFlow's large local cleanup LLM or mix speech/cloud callbacks through application-wide globals. Use deterministic low-cost cleanup and optional text-only ChatGPT enhancement. Native target validation remains mandatory regardless of UI design.
