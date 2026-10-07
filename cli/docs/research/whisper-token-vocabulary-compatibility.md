# Whisper token-vocabulary compatibility

Research date: 2026-10-01

## Question

Can issue #66 assume that every Whisper model accepted by RDE uses the same
token-ID mapping, and therefore omit vocabulary identity and compatibility
checks when reusing exact text-token IDs as a transcription prompt?

## Conclusion

No. Official Whisper models already use two incompatible text vocabularies:
English-only models use the GPT-2 vocabulary, while multilingual models use a
separate multilingual vocabulary. The same integer identifies different text
in the two vocabularies. For example, ID 258 is `he` in the GPT-2 vocabulary
but ` th` in the multilingual vocabulary. The official files also contain
50,256 and 50,257 mergeable text tokens respectively.

However, issue #66 can omit **cross-model** vocabulary identity if it is limited
to prompts passed between decode spans of one initial-transcription run. RDE
loads one model into one decoder for that run, so the producing and consuming
decoder necessarily have the same vocabulary. The broader compatibility rule
belongs to the follow-up feature that supplies an existing chunk's tokens to a
new decoder call after the model may have changed.

## Evidence

### OpenAI ships two text-token mappings

OpenAI's tokenizer selects `multilingual` for a multilingual model and `gpt2`
otherwise. Both encodings are loaded from separate `.tiktoken` rank files;
language-count changes add control tokens after the mergeable ranks rather than
selecting a third text vocabulary. See OpenAI's
[`get_encoding` and `get_tokenizer`](https://github.com/openai/whisper/blob/main/whisper/tokenizer.py#L334-L395).

The source data proves that the mappings are not interchangeable:

- [`gpt2.tiktoken`](https://raw.githubusercontent.com/openai/whisper/main/whisper/assets/gpt2.tiktoken)
  has 50,256 lines. Around ID 258 it maps `aGU=` (`he`) to 258 and
  `IHRo` (` th`) to 294.
- [`multilingual.tiktoken`](https://raw.githubusercontent.com/openai/whisper/main/whisper/assets/multilingual.tiktoken)
  has 50,257 lines. It maps `IHRo` (` th`) to 258; `aGU=` does not occupy that
  ID.

Thus even an English text token from an `.en` model cannot safely be passed by
ID to a multilingual model, or vice versa.

OpenAI lists `.en` and multilingual variants for tiny, base, small, and medium,
while large and turbo are multilingual-only. See the official
[model card](https://github.com/openai/whisper/blob/main/model-card.md#model-details)
and the official
[model registry](https://github.com/openai/whisper/blob/main/whisper/__init__.py#L17-L32).

### Large-v3 and turbo do not introduce a new text-token mapping

Large-v3 and large-v3-turbo have vocabulary size 51,866, one more than older
multilingual checkpoints because the language-token set grew. Their official
configs report that size:
[large-v3](https://huggingface.co/openai/whisper-large-v3/raw/main/config.json)
and
[large-v3-turbo](https://huggingface.co/openai/whisper-large-v3-turbo/raw/main/config.json).
OpenAI's tokenizer still selects the same `multilingual.tiktoken` mergeable
ranks and varies `num_languages` only while appending control tokens. Therefore
text-token IDs below the control-token range remain compatible across the
official multilingual generations, including large-v3 and turbo.

### Distil variants follow their source tokenizer family, not one universal family

Distil-Whisper includes `distil-medium.en` and `distil-small.en`, as well as
`distil-large-v2` and `distil-large-v3`. The project describes all of them as
English speech-transcription checkpoints, but the `.en` suffix still distinguishes
the English-tokenizer family from large-derived checkpoints. See the
[Distil-Whisper model table](https://github.com/huggingface/distil-whisper#distil-whisper).
The published `distil-large-v3` configuration has vocabulary size 51,866, the
large-v3 multilingual-family size; see its
[configuration](https://huggingface.co/distil-whisper/distil-large-v3/blob/39c4a38e135bdb24e72610f57272b046968ca87e/config.json).

The important compatibility property is the token mapping embedded in the
artifact, not whether the model is used only for English.

### RDE accepts more than the official model set

RDE pins `whisper-rs` commit
[`001016f`](https://github.com/olpa/whisper-rs/tree/001016f2c773324594327e2be47b0ef9103091d0),
whose whisper.cpp submodule is commit `5ae298e`. That whisper.cpp version
supports conversion of Hugging Face fine-tuned models. Its converter reads the
model's own `vocab.json` and writes the token strings into the ggml model, rather
than requiring one universal mapping; see
[`convert-h5-to-ggml.py`](https://github.com/ggml-org/whisper.cpp/blob/5ae298ef696d454f458a10160afcd877fff19170/models/convert-h5-to-ggml.py#L125-L184).
The pinned loader then reads the ordered vocabulary from the model file into
its ID-to-token map; see
[`whisper.cpp`](https://github.com/ggml-org/whisper.cpp/blob/5ae298ef696d454f458a10160afcd877fff19170/src/whisper.cpp#L1600-L1690).

The OpenAI-checkpoint converter also makes the split explicit: it chooses
`multilingual.tiktoken` when `n_vocab >= 51865` and `gpt2.tiktoken` otherwise,
then embeds those tokens into the model; see
[`convert-pt-to-ggml.py`](https://github.com/ggml-org/whisper.cpp/blob/5ae298ef696d454f458a10160afcd877fff19170/models/convert-pt-to-ggml.py#L227-L289).

Consequently, neither a model filename nor the fact that whisper.cpp can load a
model establishes token-ID compatibility with another model.

## Recommendation for issue #66

Drop this requirement from #66:

> Record a vocabulary identity independently from model weights. Derive it
> from the vocabulary size, token mapping, and relevant control-token IDs.

Replace it with the narrower invariant:

> During one initial-transcription run, reuse exact accepted text-token IDs
> only with the same loaded decoder that produced them.

This is sufficient because #66's sequential decode spans share one loaded
model. It avoids implementing and persisting an identity that cannot affect a
decision within that run.

Do **not** adopt the broader claim that all Whisper vocabularies are compatible.
For the follow-up feature that supplies previous-chunk text after a model
change, require the target decoder to establish compatibility before reusing
IDs. That follow-up can choose the smallest sufficient mechanism—for example,
an explicit tokenizer-family identity for supported official models, or a
comparison/hash of the actual ID-to-token mapping. It must not pass IDs between
the GPT-2 and multilingual families and must not infer compatibility solely
from model weights, model size, language setting, or filename.
