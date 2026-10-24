# Real ASR fixture

`jfk.wav` is the 11-second excerpt of President John F. Kennedy's official US
government inaugural address, 20 January 1961. The speech is a US government
work, public domain in the United States. It is included in upstream whisper.cpp
as its standard speech-recognition sample. Onda did not record or synthesize it.

- Original speech/transcript: [JFK Presidential Library](https://www.jfklibrary.org/archives/other-resources/john-f-kennedy-speeches/inaugural-address-19610120).
- Fixture provenance: [whisper.cpp sample at the commit that last changed this asset](https://github.com/ggml-org/whisper.cpp/blob/b0a11594aec50892a02cd8d129eee2dfe93a8bb8/samples/jfk.wav).
- File SHA-256: `59dfb9a4acb36fe2a2affc14bacbee2920ff435cb13cc314a08c13f66ba7860e`.
- PCM: signed 16-bit, mono, 16,000 Hz, 176,000 samples, 11.000 seconds.
- The upstream repository is MIT licensed; retained attribution: Copyright
  (c) 2023–2026 The ggml authors. The underlying official speech has its separate
  public-domain status.

The semantic golden test requires both key clauses, after punctuation/case
normalization: `ask not what your country can do for you` and
`ask what you can do for your country`. This proves meaningful recognition rather
than merely accepting native-process exit status. It is an intentionally narrow
smoke test, not an accent, noise, language or dictation-quality certification.

No models are tracked here. See `scripts/asr-benchmark.ps1` and `docs/asr.md`.
