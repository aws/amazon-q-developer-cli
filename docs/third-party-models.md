# Third-Party Model Artifacts

Kiro CLI downloads pre-trained machine-learning model artifacts at runtime
for two features: **semantic search** (embeddings) and **voice mode**
(speech-to-text). The models originate from third-party open-source projects
and are governed by their upstream licenses.

Legal attribution lives in [`../NOTICE`](../NOTICE) at the repository root
(Apache 2.0 §4(d) / MIT-compliant, attribution-only format). This file
provides the operational context: where each artifact comes from, which
files are downloaded, and which download path is used.

---

## Semantic search — embedding models

Used by the `semantic-search-client` crate. There are two independent
download paths and both are governed by the same upstream Apache 2.0
license.

### Path A — Candle backend (Amazon CDN, safetensors)

Used when the crate is built with the Candle text-embedder.

| | |
|---|---|
| Upstream | https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2 |
| License | Apache License 2.0 |
| Redistributed at | `https://desktop-release.q.us-east-1.amazonaws.com/models/all-MiniLM-L6-v2.zip` |
| Cache directory | `~/.semantic_search/models/all-MiniLM-L6-v2/` |
| Files in zip | `model.safetensors`, `tokenizer.json` |

The `all-MiniLM-L12-v2` variant is declared in
`crates/semantic-search-client/src/embedding/candle_models.rs` but is **not
currently rehosted on the Amazon CDN** (the equivalent zip URL returns HTTP
403). Only the L6 variant is reachable via this path.

### Path B — ONNX backend via `fastembed` (HuggingFace direct)

Used by default. The `fastembed` crate fetches quantized ONNX variants of
the same upstream models directly from HuggingFace on first use and caches
them locally.

| | |
|---|---|
| Upstream | https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2 (quantized ONNX conversion) |
| License | Apache License 2.0 (weights) |
| Downloaded from | HuggingFace (managed by the `fastembed` crate; no Amazon CDN involved) |
| Cache directory | `~/.semantic_search/models/` (fastembed subdirectory) |
| Model IDs | `all-MiniLM-L6-v2-Q`, `all-MiniLM-L12-v2-Q` |

Both L6 and L12 variants are available through this path. The `-Q` suffix
denotes the quantized ONNX conversion — the model weights and license
remain the upstream sentence-transformers Apache 2.0 artifact; the
quantization is a lossy transformation of those weights.

---

## Voice mode — speech-to-text model

Used by the `voice` crate. Local speech-to-text runs via `whisper.cpp`
against a GGML-format model file.

| | |
|---|---|
| Upstream code | https://github.com/openai/whisper |
| Upstream conversion | https://github.com/ggml-org/whisper.cpp |
| HuggingFace mirror | https://huggingface.co/ggerganov/whisper.cpp |
| License | MIT (both OpenAI weights and whisper.cpp conversion) |
| Redistributed at | `https://prod.download.cli.kiro.dev/stable/models/ggml-<size>.bin.zip` |
| Cache directory | `~/.kiro/voice/models/` |
| Files in zip | `ggml-<size>.bin` |

### Accepted sizes

The `voice` crate validates the configured model size against a fixed
allow-list of `["base", "small"]`
(`crates/voice/src/providers/local_whisper.rs`). Any other value falls
back to the default `"base"`. **The `medium`, `large`, and `large-v3`
sizes are not currently downloadable through this path.** The default
model is `base`.

---

If you find a model that Kiro CLI downloads at runtime and isn't listed
here, or an upstream license has changed, please open a PR against this
file and `NOTICE`.
