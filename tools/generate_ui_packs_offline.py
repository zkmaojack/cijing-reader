#!/usr/bin/env python3
"""Build embedded UI language packs with a local M2M100 model.

This utility intentionally performs no network requests. It extracts the
transport catalog and supported locale list from ``assets/web/i18n.js``, runs
batched local inference, checkpoints each completed batch, validates the exact
catalog shape and protected tokens, and finally emits:

* ``assets/web/ui-packs/<locale>.json``
* ``assets/web/ui-packs/manifest.json``
* ``assets/web/ui-language-packs.js``

Transformers example:

    python tools/generate_ui_packs_offline.py \
      --backend transformers \
      --model D:/models/facebook-m2m100_418M \
      --skip-unsupported

CTranslate2 example:

    python tools/generate_ui_packs_offline.py \
      --backend ctranslate2 \
      --model D:/models/m2m100_418M-ct2 \
      --tokenizer D:/models/facebook-m2m100_418M \
      --compute-type int8 \
      --skip-unsupported

M2M100 does not contain every locale exposed by the application. Unsupported
locales are rejected by default instead of silently receiving a wrong
language. Use ``--skip-unsupported`` to build the covered subset, then run this
tool again with a compatible local model/tokenizer and ``--locale-map`` for the
remaining locales. Existing complete packs are reused, so multiple offline
models can safely contribute to one final bundle.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable, Mapping, Protocol, Sequence


PROJECT_DIR = Path(__file__).resolve().parent.parent
DEFAULT_I18N_PATH = PROJECT_DIR / "assets" / "web" / "i18n.js"
DEFAULT_OUTPUT_DIR = PROJECT_DIR / "assets" / "web" / "ui-packs"
DEFAULT_BUNDLE_PATH = PROJECT_DIR / "assets" / "web" / "ui-language-packs.js"
DEFAULT_CHECKPOINT_DIR = PROJECT_DIR / "target" / "ui-pack-build-state"
BUILT_IN_LOCALES = frozenset({"zh-Hans", "en"})
DEFAULT_BRAND_TERMS = ("Yujie Reader", "Yujie", "语界精读")
DEFAULT_LOCALE_MAP = {
    "zh-Hans": "zh",
    "zh-Hant": "zh",
    "pt-BR": "pt",
    "pt-PT": "pt",
    "fil": "tl",
}

# These expressions are deliberately restored byte-for-byte after inference.
# The outer non-capturing group is supplied by compile_protected_pattern().
BASE_PROTECTED_PATTERNS = (
    r"__YJ_PH_\d+__",
    r"\$\{[^{}\r\n]+\}",
    r"\{[A-Za-z_][A-Za-z0-9_.-]*\}",
    r"%(?:\d+\$)?[sdif]",
    r"</?[A-Za-z][^>]*>",
    r"&(?:[A-Za-z][A-Za-z0-9]+|#\d+|#x[0-9A-Fa-f]+);",
    r"https?://[^\s<>\"']+",
    r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b",
    r"\b(?:DOCX|PDF|IPA|WebView2)\b",
    r"\bCtrl\+[A-Za-z0-9+]+\b",
)


class BuildError(RuntimeError):
    """A deterministic build or validation error."""


@dataclass(frozen=True)
class CatalogData:
    version: str
    catalog: dict[str, str]
    locales: tuple[str, ...]


@dataclass(frozen=True)
class MaskedText:
    source: str
    masked: str
    replacements: tuple[tuple[str, str], ...]


class TranslationEngine(Protocol):
    """Minimal interface shared by Transformers and CTranslate2."""

    @property
    def identity(self) -> str:
        ...

    def supports_language(self, language_code: str) -> bool:
        ...

    def translate_batch(
        self,
        texts: Sequence[str],
        target_language: str,
    ) -> list[str]:
        ...


def compact_json(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def atomic_write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(
        f".{path.name}.{os.getpid()}.{time.monotonic_ns()}.tmp"
    )
    try:
        temporary.write_text(text, encoding="utf-8", newline="\n")
        os.replace(temporary, path)
    finally:
        try:
            temporary.unlink(missing_ok=True)
        except OSError:
            pass


def load_json_file(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise BuildError(f"JSON file does not exist: {path}") from None
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise BuildError(f"Could not read JSON from {path}: {error}") from error


def extract_catalog_with_node(i18n_path: Path, node_binary: str) -> CatalogData:
    """Evaluate i18n.js in a sealed Node VM and capture its transport request."""

    extractor = r"""
const fs = require("node:fs");
const vm = require("node:vm");
const sourcePath = process.argv[1];
let captured = null;
const storage = new Map();
const sandbox = {
  AbortController,
  clearTimeout,
  console: { error() {}, log() {}, warn() {} },
  document: undefined,
  localStorage: {
    getItem(key) { return storage.has(key) ? storage.get(key) : null; },
    removeItem(key) { storage.delete(key); },
    setItem(key, value) { storage.set(key, String(value)); },
  },
  setTimeout,
};
sandbox.window = sandbox;
sandbox.fetch = async (_url, options) => {
  captured = JSON.parse(String(options && options.body || "{}"));
  return {
    ok: true,
    status: 200,
    async json() {
      return {
        ok: true,
        locale: captured.locale,
        version: captured.version,
        pack: JSON.parse(captured.catalog),
      };
    },
  };
};
try {
  vm.runInNewContext(fs.readFileSync(sourcePath, "utf8"), sandbox, {
    filename: sourcePath,
  });
  const api = sandbox.YujieI18n;
  if (
    api &&
    typeof api.exportCatalog === "function" &&
    typeof api.CATALOG_VERSION === "string" &&
    Array.isArray(api.packLocales || api.supportedLocales)
  ) {
    process.stdout.write(JSON.stringify({
      version: api.CATALOG_VERSION,
      catalog: api.exportCatalog({ masked: false }),
      locales: api.packLocales || api.supportedLocales,
    }));
  } else {
    Promise.resolve(sandbox.YujieI18n.setLocale("es", { apply: false }))
      .then(() => {
        if (!captured || !captured.catalog || !captured.version) {
          throw new Error("i18n.js did not emit a catalog request");
        }
        process.stdout.write(JSON.stringify({
          version: captured.version,
          catalog: JSON.parse(captured.catalog),
          locales: sandbox.YujieI18n.supportedLocales,
        }));
      })
      .catch((error) => {
        process.stderr.write(String(error && error.stack || error));
        process.exitCode = 1;
      });
  }
} catch (error) {
  process.stderr.write(String(error && error.stack || error));
  process.exitCode = 1;
}
"""
    try:
        completed = subprocess.run(
            [node_binary, "-e", extractor, str(i18n_path)],
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=60,
        )
    except FileNotFoundError:
        raise BuildError(
            f"Node.js executable was not found ({node_binary!r}); "
            "install Node.js or pass --catalog-json"
        ) from None
    except subprocess.TimeoutExpired:
        raise BuildError("Timed out while extracting the catalog from i18n.js") from None

    if completed.returncode != 0:
        details = completed.stderr.strip() or completed.stdout.strip()
        raise BuildError(f"Could not extract i18n catalog: {details}")
    try:
        payload = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise BuildError(f"Catalog extractor returned invalid JSON: {error}") from error
    return parse_catalog_payload(payload, source=str(i18n_path))


def parse_catalog_payload(payload: Any, source: str) -> CatalogData:
    if not isinstance(payload, dict):
        raise BuildError(f"Catalog payload from {source} must be an object")
    version = payload.get("version")
    catalog = payload.get("catalog")
    locales = payload.get("locales")
    if not isinstance(version, str) or not version.strip():
        raise BuildError(f"Catalog payload from {source} has no version")
    if not isinstance(catalog, dict) or not catalog:
        raise BuildError(f"Catalog payload from {source} has no catalog")
    if not isinstance(locales, list) or not locales:
        raise BuildError(f"Catalog payload from {source} has no locale list")

    normalized_catalog: dict[str, str] = {}
    for key, value in catalog.items():
        if not isinstance(key, str) or not key:
            raise BuildError(f"Catalog payload from {source} contains an invalid key")
        if not isinstance(value, str) or not value.strip():
            raise BuildError(f"Catalog entry {key!r} is empty or not text")
        normalized_catalog[key] = value
    normalized_locales = tuple(
        dict.fromkeys(
            locale
            for locale in locales
            if isinstance(locale, str) and locale.strip()
        )
    )
    if not normalized_locales:
        raise BuildError(f"Catalog payload from {source} has no valid locale codes")
    return CatalogData(version.strip(), normalized_catalog, normalized_locales)


def load_catalog(args: argparse.Namespace) -> CatalogData:
    if args.catalog_json:
        return parse_catalog_payload(
            load_json_file(args.catalog_json),
            source=str(args.catalog_json),
        )
    return extract_catalog_with_node(args.i18n, args.node)


def parse_locale_list(values: Sequence[str] | None) -> tuple[str, ...]:
    if not values:
        return ()
    result: list[str] = []
    for value in values:
        for locale in value.split(","):
            locale = locale.strip()
            if locale and locale not in result:
                result.append(locale)
    return tuple(result)


def load_locale_map(path: Path | None) -> dict[str, str]:
    result = dict(DEFAULT_LOCALE_MAP)
    if path is None:
        return result
    payload = load_json_file(path)
    if not isinstance(payload, dict):
        raise BuildError("--locale-map must contain a JSON object")
    for locale, model_code in payload.items():
        if (
            not isinstance(locale, str)
            or not locale.strip()
            or not isinstance(model_code, str)
            or not model_code.strip()
        ):
            raise BuildError("--locale-map keys and values must be non-empty strings")
        result[locale.strip()] = model_code.strip()
    return result


def model_language_for(locale: str, locale_map: Mapping[str, str]) -> str:
    return locale_map.get(locale, locale.split("-", 1)[0])


def compile_protected_pattern(extra_terms: Sequence[str]) -> re.Pattern[str]:
    terms = tuple(
        dict.fromkeys(
            term.strip()
            for term in (*DEFAULT_BRAND_TERMS, *extra_terms)
            if term.strip()
        )
    )
    term_patterns = [
        re.escape(term)
        for term in sorted(terms, key=lambda value: (-len(value), value))
    ]
    patterns = (*BASE_PROTECTED_PATTERNS, *term_patterns)
    return re.compile(f"(?:{'|'.join(patterns)})", re.IGNORECASE)


def mask_protected_text(source: str, pattern: re.Pattern[str]) -> MaskedText:
    replacements: list[tuple[str, str]] = []

    def replace(match: re.Match[str]) -> str:
        token = f"__YJ_KEEP_{len(replacements):03d}__"
        replacements.append((token, match.group(0)))
        return token

    return MaskedText(source, pattern.sub(replace, source), tuple(replacements))


def restore_masked_text(masked: MaskedText, translated: str) -> str | None:
    result = translated.strip()
    for token, original in masked.replacements:
        if result.count(token) != 1:
            return None
        result = result.replace(token, original)
    if "__YJ_KEEP_" in result:
        return None
    return result.strip() or None


def split_protected_text(
    source: str,
    pattern: re.Pattern[str],
) -> tuple[list[str], list[str]]:
    """Return translatable spans and protected separators in source order."""

    spans: list[str] = []
    protected: list[str] = []
    cursor = 0
    for match in pattern.finditer(source):
        spans.append(source[cursor : match.start()])
        protected.append(match.group(0))
        cursor = match.end()
    spans.append(source[cursor:])
    return spans, protected


def split_surrounding_whitespace(text: str) -> tuple[str, str, str]:
    match = re.fullmatch(r"(\s*)(.*?)(\s*)", text, flags=re.DOTALL)
    assert match is not None
    return match.group(1), match.group(2), match.group(3)


def translate_unique(
    engine: TranslationEngine,
    texts: Sequence[str],
    target_language: str,
    batch_size: int,
) -> dict[str, str]:
    unique = list(dict.fromkeys(text for text in texts if text.strip()))
    translated: dict[str, str] = {}
    for offset in range(0, len(unique), batch_size):
        batch = unique[offset : offset + batch_size]
        outputs = engine.translate_batch(batch, target_language)
        if len(outputs) != len(batch):
            raise BuildError(
                f"Translation engine returned {len(outputs)} result(s) "
                f"for a batch of {len(batch)}"
            )
        for source, output in zip(batch, outputs):
            if not isinstance(output, str) or not output.strip():
                # Punctuation-only and symbol-heavy labels can legitimately
                # decode to an empty sequence. Keeping the source is safer than
                # failing an otherwise complete offline language pack.
                translated[source] = source
            else:
                translated[source] = output.strip()
    return translated


def translate_records(
    engine: TranslationEngine,
    sources: Sequence[str],
    target_language: str,
    batch_size: int,
    protected_pattern: re.Pattern[str],
) -> list[str]:
    """Translate records with a contextual fast path and exact-token fallback."""

    masked_records = [
        mask_protected_text(source, protected_pattern) for source in sources
    ]
    contextual = translate_unique(
        engine,
        [record.masked for record in masked_records],
        target_language,
        batch_size,
    )
    results: list[str | None] = []
    failed_indexes: list[int] = []
    for index, record in enumerate(masked_records):
        restored = restore_masked_text(record, contextual[record.masked])
        results.append(restored)
        if restored is None:
            failed_indexes.append(index)

    if failed_indexes:
        # Some models rewrite sentinel punctuation. Fall back to translating only
        # the unprotected fragments, which makes corruption impossible.
        fragments_by_index: dict[int, tuple[list[str], list[str]]] = {}
        translatable_cores: list[str] = []
        for index in failed_indexes:
            spans, protected = split_protected_text(
                sources[index],
                protected_pattern,
            )
            fragments_by_index[index] = (spans, protected)
            for span in spans:
                _prefix, core, _suffix = split_surrounding_whitespace(span)
                if core:
                    translatable_cores.append(core)
        translated_cores = translate_unique(
            engine,
            translatable_cores,
            target_language,
            batch_size,
        )
        for index in failed_indexes:
            spans, protected = fragments_by_index[index]
            rebuilt: list[str] = []
            for span_index, span in enumerate(spans):
                prefix, core, suffix = split_surrounding_whitespace(span)
                rebuilt.append(
                    f"{prefix}{translated_cores.get(core, core)}{suffix}"
                )
                if span_index < len(protected):
                    rebuilt.append(protected[span_index])
            results[index] = "".join(rebuilt).strip()

    if any(result is None for result in results):
        raise BuildError("Could not restore protected text after translation")
    return [str(result) for result in results]


def protected_counter(text: str, pattern: re.Pattern[str]) -> Counter[str]:
    return Counter(match.group(0) for match in pattern.finditer(text))


def validate_translation(
    key: str,
    source: str,
    translated: Any,
    protected_pattern: re.Pattern[str],
) -> str:
    if not isinstance(translated, str) or not translated.strip():
        raise BuildError(f"{key}: translation is empty or not text")
    translated = translated.strip()
    maximum_length = max(512, len(source) * 12 + 128)
    if len(translated) > maximum_length:
        raise BuildError(
            f"{key}: translation length {len(translated)} exceeds {maximum_length}"
        )
    source_tokens = protected_counter(source, protected_pattern)
    translated_tokens = protected_counter(translated, protected_pattern)
    if source_tokens != translated_tokens:
        missing = source_tokens - translated_tokens
        added = translated_tokens - source_tokens
        raise BuildError(
            f"{key}: protected tokens changed "
            f"(missing={dict(missing)}, added={dict(added)})"
        )
    return translated


def validate_pack_payload(
    payload: Any,
    locale: str,
    catalog_data: CatalogData,
    protected_pattern: re.Pattern[str],
) -> dict[str, str]:
    if not isinstance(payload, dict):
        raise BuildError(f"{locale}: language pack must be an object")
    if payload.get("locale") != locale:
        raise BuildError(f"{locale}: language pack locale does not match")
    if payload.get("version") != catalog_data.version:
        raise BuildError(f"{locale}: language pack catalog version is stale")
    pack = payload.get("pack")
    if not isinstance(pack, dict):
        raise BuildError(f"{locale}: language pack has no pack object")
    expected_keys = list(catalog_data.catalog)
    actual_keys = list(pack)
    if set(actual_keys) != set(expected_keys) or len(actual_keys) != len(expected_keys):
        missing = [key for key in expected_keys if key not in pack]
        extra = [key for key in actual_keys if key not in catalog_data.catalog]
        raise BuildError(
            f"{locale}: expected {len(expected_keys)} exact keys, got "
            f"{len(actual_keys)} (missing={missing[:5]}, extra={extra[:5]})"
        )
    validated: dict[str, str] = {}
    for key, source in catalog_data.catalog.items():
        validated[key] = validate_translation(
            key,
            source,
            pack[key],
            protected_pattern,
        )
    return validated


def read_complete_pack(
    path: Path,
    locale: str,
    catalog_data: CatalogData,
    protected_pattern: re.Pattern[str],
) -> dict[str, str] | None:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
        return validate_pack_payload(
            payload,
            locale,
            catalog_data,
            protected_pattern,
        )
    except (OSError, UnicodeError, json.JSONDecodeError, BuildError):
        return None


def pack_payload(
    locale: str,
    version: str,
    pack: Mapping[str, str],
) -> dict[str, object]:
    return {"locale": locale, "version": version, "pack": dict(pack)}


def checkpoint_signature(
    catalog_data: CatalogData,
    engine: TranslationEngine,
    locale: str,
    target_language: str,
    zh_hant_mode: str,
) -> str:
    source_hash = hashlib.sha256(
        compact_json(catalog_data.catalog).encode("utf-8")
    ).hexdigest()
    identity = compact_json(
        {
            "catalogVersion": catalog_data.version,
            "sourceHash": source_hash,
            "engine": engine.identity,
            "locale": locale,
            "targetLanguage": target_language,
            "zhHantMode": zh_hant_mode,
        }
    )
    return hashlib.sha256(identity.encode("utf-8")).hexdigest()


def read_checkpoint(
    path: Path,
    signature: str,
    catalog_data: CatalogData,
    protected_pattern: re.Pattern[str],
) -> dict[str, str]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        return {}
    if not isinstance(payload, dict) or payload.get("signature") != signature:
        return {}
    completed = payload.get("completed")
    if not isinstance(completed, dict):
        return {}
    valid: dict[str, str] = {}
    for key, value in completed.items():
        source = catalog_data.catalog.get(key)
        if source is None:
            continue
        try:
            valid[key] = validate_translation(
                key,
                source,
                value,
                protected_pattern,
            )
        except BuildError:
            continue
    return valid


def write_checkpoint(
    path: Path,
    signature: str,
    completed: Mapping[str, str],
) -> None:
    atomic_write_text(
        path,
        f"{compact_json({'signature': signature, 'completed': dict(completed)})}\n",
    )


class TraditionalChineseConverter:
    def __init__(self, mode: str, needed: bool) -> None:
        self._converter: Any = None
        if not needed or mode == "none":
            return
        try:
            from opencc import OpenCC
        except ImportError:
            raise BuildError(
                "zh-Hant generation requires OpenCC. Install "
                "'opencc-python-reimplemented', or explicitly pass "
                "--zh-hant-mode none if the local model already emits "
                "Traditional Chinese."
            ) from None
        last_error: Exception | None = None
        for configuration in ("s2twp", "s2twp.json", "s2t"):
            try:
                self._converter = OpenCC(configuration)
                break
            except Exception as error:  # Different OpenCC packages use variants.
                last_error = error
        if self._converter is None:
            raise BuildError(f"Could not initialize OpenCC: {last_error}")

    def convert(self, locale: str, text: str) -> str:
        if locale == "zh-Hant" and self._converter is not None:
            return str(self._converter.convert(text))
        return text


class TransformersM2M100Engine:
    def __init__(self, args: argparse.Namespace) -> None:
        try:
            import torch
            from transformers import (
                M2M100ForConditionalGeneration,
                M2M100Tokenizer,
            )
        except ImportError:
            raise BuildError(
                "The Transformers backend needs local dependencies: "
                "torch, transformers, and sentencepiece"
            ) from None

        device = args.device
        if device == "auto":
            device = "cuda" if torch.cuda.is_available() else "cpu"
        if device == "cuda" and not torch.cuda.is_available():
            raise BuildError("CUDA was requested but torch.cuda.is_available() is false")

        dtype_by_name = {
            "float16": torch.float16,
            "float32": torch.float32,
            "bfloat16": torch.bfloat16,
        }
        dtype_name = args.dtype
        if dtype_name == "auto":
            dtype_name = "float16" if device == "cuda" else "float32"
        if device == "cpu" and dtype_name == "float16":
            raise BuildError("float16 inference is not supported on CPU")

        model_path = str(args.model)
        tokenizer_path = str(args.tokenizer or args.model)
        try:
            self._tokenizer = M2M100Tokenizer.from_pretrained(
                tokenizer_path,
                local_files_only=True,
            )
            self._model = M2M100ForConditionalGeneration.from_pretrained(
                model_path,
                local_files_only=True,
                torch_dtype=dtype_by_name[dtype_name],
            )
        except Exception as error:
            raise BuildError(f"Could not load the local M2M100 model: {error}") from error
        self._torch = torch
        self._device = device
        self._source_language = args.source_language
        self._beam_size = args.beam_size
        self._max_source_length = args.max_source_length
        self._max_decoding_length = args.max_decoding_length
        self._model.to(device)
        self._model.eval()
        self._identity = compact_json(
            {
                "backend": "transformers",
                "model": str(args.model.resolve()),
                "tokenizer": str((args.tokenizer or args.model).resolve()),
                "device": device,
                "dtype": dtype_name,
                "beamSize": self._beam_size,
            }
        )
        if not self.supports_language(self._source_language):
            raise BuildError(
                f"The tokenizer does not support source language "
                f"{self._source_language!r}"
            )

    @property
    def identity(self) -> str:
        return self._identity

    def supports_language(self, language_code: str) -> bool:
        return language_code in getattr(self._tokenizer, "lang_code_to_id", {})

    def translate_batch(
        self,
        texts: Sequence[str],
        target_language: str,
    ) -> list[str]:
        self._tokenizer.src_lang = self._source_language
        encoded = self._tokenizer(
            list(texts),
            return_tensors="pt",
            padding=True,
            truncation=True,
            max_length=self._max_source_length,
        )
        encoded = {
            key: value.to(self._device)
            for key, value in encoded.items()
        }
        target_id = self._tokenizer.get_lang_id(target_language)
        with self._torch.inference_mode():
            generated = self._model.generate(
                **encoded,
                forced_bos_token_id=target_id,
                num_beams=self._beam_size,
                max_new_tokens=self._max_decoding_length,
            )
        return [
            text.strip()
            for text in self._tokenizer.batch_decode(
                generated,
                skip_special_tokens=True,
            )
        ]


class CTranslate2M2M100Engine:
    def __init__(self, args: argparse.Namespace) -> None:
        if args.tokenizer is None:
            raise BuildError(
                "The CTranslate2 backend requires --tokenizer pointing to the "
                "original local Hugging Face M2M100 tokenizer directory"
            )
        try:
            import ctranslate2
            from transformers import M2M100Tokenizer
        except ImportError:
            raise BuildError(
                "The CTranslate2 backend needs local dependencies: "
                "ctranslate2, transformers, and sentencepiece"
            ) from None

        device = args.device
        if device == "auto":
            device = (
                "cuda"
                if ctranslate2.get_cuda_device_count() > 0
                else "cpu"
            )
        try:
            self._tokenizer = M2M100Tokenizer.from_pretrained(
                str(args.tokenizer),
                local_files_only=True,
            )
            self._translator = ctranslate2.Translator(
                str(args.model),
                device=device,
                compute_type=args.compute_type,
                inter_threads=args.inter_threads,
                intra_threads=args.intra_threads,
            )
        except Exception as error:
            raise BuildError(
                f"Could not load the local CTranslate2 M2M100 model: {error}"
            ) from error
        self._source_language = args.source_language
        self._beam_size = args.beam_size
        self._max_source_length = args.max_source_length
        self._max_decoding_length = args.max_decoding_length
        self._identity = compact_json(
            {
                "backend": "ctranslate2",
                "model": str(args.model.resolve()),
                "tokenizer": str(args.tokenizer.resolve()),
                "device": device,
                "computeType": args.compute_type,
                "beamSize": self._beam_size,
            }
        )
        if not self.supports_language(self._source_language):
            raise BuildError(
                f"The tokenizer does not support source language "
                f"{self._source_language!r}"
            )

    @property
    def identity(self) -> str:
        return self._identity

    def supports_language(self, language_code: str) -> bool:
        return language_code in getattr(self._tokenizer, "lang_code_to_id", {})

    def translate_batch(
        self,
        texts: Sequence[str],
        target_language: str,
    ) -> list[str]:
        self._tokenizer.src_lang = self._source_language
        encoded = self._tokenizer(
            list(texts),
            add_special_tokens=True,
            truncation=True,
            max_length=self._max_source_length,
        )["input_ids"]
        source_tokens = [
            self._tokenizer.convert_ids_to_tokens(input_ids)
            for input_ids in encoded
        ]
        target_id = self._tokenizer.get_lang_id(target_language)
        target_token = self._tokenizer.convert_ids_to_tokens(target_id)
        results = self._translator.translate_batch(
            source_tokens,
            target_prefix=[[target_token] for _ in source_tokens],
            beam_size=self._beam_size,
            max_decoding_length=self._max_decoding_length,
        )
        decoded: list[str] = []
        for result in results:
            tokens = list(result.hypotheses[0])
            if tokens and tokens[0] == target_token:
                tokens.pop(0)
            token_ids = self._tokenizer.convert_tokens_to_ids(tokens)
            decoded.append(
                self._tokenizer.decode(
                    token_ids,
                    skip_special_tokens=True,
                ).strip()
            )
        return decoded


def create_engine(args: argparse.Namespace) -> TranslationEngine:
    if args.backend == "transformers":
        return TransformersM2M100Engine(args)
    if args.backend == "ctranslate2":
        return CTranslate2M2M100Engine(args)
    raise BuildError(f"Unknown backend: {args.backend}")


def build_locale(
    locale: str,
    target_language: str,
    engine: TranslationEngine,
    catalog_data: CatalogData,
    args: argparse.Namespace,
    protected_pattern: re.Pattern[str],
    traditional_converter: TraditionalChineseConverter,
) -> dict[str, str]:
    checkpoint_path = args.checkpoint_dir / f"{locale}.json"
    signature = checkpoint_signature(
        catalog_data,
        engine,
        locale,
        target_language,
        args.zh_hant_mode,
    )
    completed = (
        {}
        if args.force
        else read_checkpoint(
            checkpoint_path,
            signature,
            catalog_data,
            protected_pattern,
        )
    )
    keys = list(catalog_data.catalog)
    pending_keys = [key for key in keys if key not in completed]
    if completed:
        print(f"  {locale}: resumed {len(completed)}/{len(keys)} strings")

    for offset in range(0, len(pending_keys), args.batch_size):
        batch_keys = pending_keys[offset : offset + args.batch_size]
        batch_number = offset // args.batch_size + 1
        batch_count = max(
            1,
            (len(pending_keys) + args.batch_size - 1) // args.batch_size,
        )
        print(
            f"  {locale}: batch {batch_number}/{batch_count} "
            f"({len(batch_keys)} strings)",
            flush=True,
        )
        sources = [catalog_data.catalog[key] for key in batch_keys]
        outputs = translate_records(
            engine,
            sources,
            target_language,
            args.batch_size,
            protected_pattern,
        )
        for key, source, output in zip(batch_keys, sources, outputs):
            if locale == "zh-Hant":
                protected_output = mask_protected_text(output, protected_pattern)
                converted_output = traditional_converter.convert(
                    locale,
                    protected_output.masked,
                )
                restored_output = restore_masked_text(
                    protected_output,
                    converted_output,
                )
                if restored_output is None:
                    raise BuildError(
                        f"{key}: Traditional Chinese conversion changed "
                        "a protected token"
                    )
                output = restored_output
            else:
                output = traditional_converter.convert(locale, output)
            try:
                completed[key] = validate_translation(
                    key,
                    source,
                    output,
                    protected_pattern,
                )
            except BuildError as error:
                # Small multilingual models occasionally hallucinate on terse
                # UI labels. A visible English source label is a deterministic
                # and safe fallback; malformed or token-corrupt text is not.
                print(
                    f"  {locale}/{key}: using source fallback ({error})",
                    flush=True,
                )
                completed[key] = validate_translation(
                    key,
                    source,
                    source,
                    protected_pattern,
                )
        write_checkpoint(checkpoint_path, signature, completed)

    ordered = {key: completed[key] for key in keys}
    validated = validate_pack_payload(
        pack_payload(locale, catalog_data.version, ordered),
        locale,
        catalog_data,
        protected_pattern,
    )
    destination = args.output_dir / f"{locale}.json"
    atomic_write_text(
        destination,
        f"{compact_json(pack_payload(locale, catalog_data.version, validated))}\n",
    )
    try:
        checkpoint_path.unlink(missing_ok=True)
    except OSError:
        pass
    return validated


def collect_complete_packs(
    catalog_data: CatalogData,
    expected_locales: Sequence[str],
    output_dir: Path,
    protected_pattern: re.Pattern[str],
) -> tuple[dict[str, dict[str, str]], list[str]]:
    packs: dict[str, dict[str, str]] = {}
    missing: list[str] = []
    for locale in expected_locales:
        pack = read_complete_pack(
            output_dir / f"{locale}.json",
            locale,
            catalog_data,
            protected_pattern,
        )
        if pack is None:
            missing.append(locale)
        else:
            packs[locale] = pack
    return packs, missing


def emit_manifest_and_bundle(
    args: argparse.Namespace,
    catalog_data: CatalogData,
    expected_locales: Sequence[str],
    protected_pattern: re.Pattern[str],
) -> list[str]:
    packs, missing = collect_complete_packs(
        catalog_data,
        expected_locales,
        args.output_dir,
        protected_pattern,
    )
    manifest = {
        "version": catalog_data.version,
        "keyCount": len(catalog_data.catalog),
        "locales": list(packs),
    }
    atomic_write_text(
        args.output_dir / "manifest.json",
        f"{compact_json(manifest)}\n",
    )
    if missing:
        print(
            f"Bundle not emitted: {len(packs)}/{len(expected_locales)} "
            f"locale(s) complete; missing {', '.join(missing)}",
            flush=True,
        )
        return missing

    bundle = {"version": catalog_data.version, "packs": packs}
    bundle_json = compact_json(bundle)
    atomic_write_text(
        args.bundle,
        f"window.YujieUiLanguagePacks={bundle_json};\n",
    )
    print(
        f"Embedded bundle ready: {len(packs)} locale(s), "
        f"{len(bundle_json.encode('utf-8'))} bytes",
        flush=True,
    )
    return []


def build_argument_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Generate complete embedded UI language packs with a local "
            "M2M100 model. This command never accesses the network."
        ),
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--backend",
        choices=("transformers", "ctranslate2"),
        default="transformers",
        help="local inference runtime",
    )
    parser.add_argument(
        "--model",
        type=Path,
        help="local Transformers model or converted CTranslate2 model directory",
    )
    parser.add_argument(
        "--tokenizer",
        type=Path,
        help="local tokenizer directory (required for CTranslate2)",
    )
    parser.add_argument(
        "--i18n",
        type=Path,
        default=DEFAULT_I18N_PATH,
        help="source i18n.js",
    )
    parser.add_argument(
        "--catalog-json",
        type=Path,
        help=(
            "optional pre-extracted {version,catalog,locales} JSON; "
            "bypasses Node.js extraction"
        ),
    )
    parser.add_argument(
        "--node",
        default="node",
        help="Node.js executable used only to extract i18n.js",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=DEFAULT_OUTPUT_DIR,
        help="per-locale language pack directory",
    )
    parser.add_argument(
        "--bundle",
        type=Path,
        default=DEFAULT_BUNDLE_PATH,
        help="final JavaScript bundle path",
    )
    parser.add_argument(
        "--checkpoint-dir",
        type=Path,
        default=DEFAULT_CHECKPOINT_DIR,
        help="batch checkpoint directory",
    )
    parser.add_argument(
        "--locales",
        action="append",
        help="comma-separated app locale codes; may be repeated",
    )
    parser.add_argument(
        "--locale-map",
        type=Path,
        help="JSON object mapping app locales to tokenizer language codes",
    )
    parser.add_argument(
        "--source-language",
        default="en",
        help="M2M100 tokenizer source language code",
    )
    parser.add_argument(
        "--batch-size",
        type=int,
        default=24,
        help="maximum strings per inference batch",
    )
    parser.add_argument(
        "--beam-size",
        type=int,
        default=4,
        help="generation beam size",
    )
    parser.add_argument(
        "--max-source-length",
        type=int,
        default=256,
        help="maximum source token count",
    )
    parser.add_argument(
        "--max-decoding-length",
        type=int,
        default=256,
        help="maximum generated token count",
    )
    parser.add_argument(
        "--device",
        choices=("auto", "cpu", "cuda"),
        default="auto",
        help="inference device",
    )
    parser.add_argument(
        "--dtype",
        choices=("auto", "float16", "float32", "bfloat16"),
        default="auto",
        help="Transformers model dtype",
    )
    parser.add_argument(
        "--compute-type",
        default="default",
        help="CTranslate2 compute type, for example int8 or float16",
    )
    parser.add_argument(
        "--inter-threads",
        type=int,
        default=1,
        help="CTranslate2 parallel translator workers",
    )
    parser.add_argument(
        "--intra-threads",
        type=int,
        default=0,
        help="CTranslate2 CPU threads per translator",
    )
    parser.add_argument(
        "--protect-term",
        action="append",
        default=[],
        help="additional literal term to preserve exactly; may be repeated",
    )
    parser.add_argument(
        "--zh-hant-mode",
        choices=("opencc", "none"),
        default="opencc",
        help=(
            "convert M2M100 zh output to Traditional Chinese with local OpenCC; "
            "use none only with a model that already emits Traditional Chinese"
        ),
    )
    parser.add_argument(
        "--skip-unsupported",
        action="store_true",
        help="build supported locales and report unsupported ones instead of failing",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="regenerate selected packs even when a complete pack already exists",
    )
    parser.add_argument(
        "--validate-only",
        action="store_true",
        help="validate existing packs and emit the final bundle without loading a model",
    )
    parser.add_argument(
        "--preflight-only",
        action="store_true",
        help="load the local model and report locale coverage without inference",
    )
    return parser


def validate_arguments(args: argparse.Namespace) -> None:
    positive_fields = (
        "batch_size",
        "beam_size",
        "max_source_length",
        "max_decoding_length",
    )
    for field in positive_fields:
        if getattr(args, field) <= 0:
            raise BuildError(f"--{field.replace('_', '-')} must be greater than zero")
    if args.inter_threads <= 0:
        raise BuildError("--inter-threads must be greater than zero")
    if args.intra_threads < 0:
        raise BuildError("--intra-threads cannot be negative")
    if not args.validate_only:
        if args.model is None:
            raise BuildError("--model is required unless --validate-only is used")
        if not args.model.exists():
            raise BuildError(f"Local model directory does not exist: {args.model}")
        if args.tokenizer is not None and not args.tokenizer.exists():
            raise BuildError(
                f"Local tokenizer directory does not exist: {args.tokenizer}"
            )


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_argument_parser()
    args = parser.parse_args(argv)
    try:
        validate_arguments(args)
        catalog_data = load_catalog(args)
        expected_locales = tuple(
            locale
            for locale in catalog_data.locales
            if locale not in BUILT_IN_LOCALES
        )
        selected = parse_locale_list(args.locales)
        unknown = [locale for locale in selected if locale not in expected_locales]
        if unknown:
            raise BuildError(
                f"Unknown or built-in locale(s): {', '.join(unknown)}"
            )
        selected_locales = selected or expected_locales
        protected_pattern = compile_protected_pattern(args.protect_term)
        args.output_dir.mkdir(parents=True, exist_ok=True)

        print(
            f"Catalog {catalog_data.version}: "
            f"{len(catalog_data.catalog)} strings, "
            f"{len(expected_locales)} embeddable locale(s)",
            flush=True,
        )

        if args.validate_only:
            missing = emit_manifest_and_bundle(
                args,
                catalog_data,
                expected_locales,
                protected_pattern,
            )
            return 2 if missing else 0

        locale_map = load_locale_map(args.locale_map)
        engine = create_engine(args)
        targets = {
            locale: model_language_for(locale, locale_map)
            for locale in selected_locales
        }
        unsupported = [
            locale
            for locale, target in targets.items()
            if not engine.supports_language(target)
        ]
        if unsupported:
            details = ", ".join(
                f"{locale}->{targets[locale]}" for locale in unsupported
            )
            if not args.skip_unsupported:
                raise BuildError(
                    "The local tokenizer does not support: "
                    f"{details}. Supply a compatible model/--locale-map, or "
                    "use --skip-unsupported and complete those locales in a "
                    "later offline run."
                )
            print(f"Skipping unsupported locale(s): {details}", flush=True)
        buildable = [
            locale for locale in selected_locales if locale not in unsupported
        ]
        traditional_converter = TraditionalChineseConverter(
            args.zh_hant_mode,
            needed="zh-Hant" in buildable,
        )

        print(
            f"Model coverage: {len(buildable)}/{len(selected_locales)} "
            f"selected locale(s)",
            flush=True,
        )
        if args.preflight_only:
            return 0 if not unsupported else 2

        for index, locale in enumerate(buildable, start=1):
            destination = args.output_dir / f"{locale}.json"
            existing = (
                None
                if args.force
                else read_complete_pack(
                    destination,
                    locale,
                    catalog_data,
                    protected_pattern,
                )
            )
            if existing is not None:
                print(
                    f"[{index}/{len(buildable)}] {locale}: already complete",
                    flush=True,
                )
                continue
            print(
                f"[{index}/{len(buildable)}] {locale}: "
                f"translating to {targets[locale]}",
                flush=True,
            )
            pack = build_locale(
                locale,
                targets[locale],
                engine,
                catalog_data,
                args,
                protected_pattern,
                traditional_converter,
            )
            size = len(
                compact_json(
                    pack_payload(locale, catalog_data.version, pack)
                ).encode("utf-8")
            )
            print(
                f"[{index}/{len(buildable)}] {locale}: "
                f"saved {len(pack)} strings, {size} bytes",
                flush=True,
            )

        missing = emit_manifest_and_bundle(
            args,
            catalog_data,
            expected_locales,
            protected_pattern,
        )
        # A deliberately selected subset is a successful resumable step.
        if selected:
            return 0
        return 2 if missing else 0
    except BuildError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        print(
            "\nInterrupted; completed batches are checkpointed and will resume.",
            file=sys.stderr,
        )
        return 130


if __name__ == "__main__":
    raise SystemExit(main())
