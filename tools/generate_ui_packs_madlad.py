#!/usr/bin/env python3
"""Build the UI locales missing from M2M100 with local MADLAD-400 CT2.

This is a thin backend adapter for ``generate_ui_packs_offline.py``.  It keeps
that tool's catalog extraction, placeholder protection, checkpoints,
per-locale JSON validation, manifest generation, and final JavaScript bundle
format.  The only model-specific difference is inference:

* M2M100 selects the target with a decoder ``target_prefix``.
* MADLAD-400 prepends ``<2xx>`` to every source sentence.

The command is offline-only.  Model and tokenizer sources must be local;
``--tokenizer`` may be omitted when those files are stored in the CTranslate2
model directory.

Example:

    python tools/generate_ui_packs_madlad.py \
      --model D:/models/madlad400-3b-mt-ct2-int8 \
      --compute-type int8 \
      --device cpu

By default only the 13 application locales unsupported by M2M100 are built:
``mi bo ug te ky tg tk eu mt ku om rw ny``.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Sequence

import generate_ui_packs_offline as offline


MADLAD_UI_LOCALE_MAP = {
    "mi": "mi",  # Maori
    "bo": "bo",  # Tibetan
    "ug": "ug",  # Uyghur
    "te": "te",  # Telugu
    "ky": "ky",  # Kyrgyz
    "tg": "tg",  # Tajik
    "tk": "tk",  # Turkmen
    "eu": "eu",  # Basque
    "mt": "mt",  # Maltese
    "ku": "ku",  # Kurdish
    "om": "om",  # Oromo
    "rw": "rw",  # Kinyarwanda
    "ny": "ny",  # Chichewa
}
DEFAULT_MADLAD_LOCALES = tuple(MADLAD_UI_LOCALE_MAP)

_BASE_BUILD_ARGUMENT_PARSER = offline.build_argument_parser


class CTranslate2MadladEngine:
    """MADLAD-400 translation through CTranslate2 and the HF T5 tokenizer."""

    def __init__(self, args: argparse.Namespace) -> None:
        tokenizer_path = args.tokenizer or args.model
        try:
            import ctranslate2
            from transformers import T5Tokenizer
        except ImportError:
            raise offline.BuildError(
                "The MADLAD backend needs local dependencies: "
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
            self._tokenizer = T5Tokenizer.from_pretrained(
                str(tokenizer_path),
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
            raise offline.BuildError(
                f"Could not load the local CTranslate2 MADLAD-400 model: {error}"
            ) from error

        self._beam_size = args.beam_size
        self._max_source_length = args.max_source_length
        self._max_decoding_length = args.max_decoding_length
        self._identity = offline.compact_json(
            {
                "backend": "ctranslate2-madlad400",
                "model": str(args.model.resolve()),
                "tokenizer": str(Path(tokenizer_path).resolve()),
                "device": device,
                "computeType": args.compute_type,
                "beamSize": self._beam_size,
                "targetProtocol": "source-prefix:<2xx>",
            }
        )

    @property
    def identity(self) -> str:
        return self._identity

    @staticmethod
    def target_tag(language_code: str) -> str:
        return f"<2{language_code}>"

    def supports_language(self, language_code: str) -> bool:
        tag = self.target_tag(language_code)
        token_id = self._tokenizer.convert_tokens_to_ids(tag)
        return (
            isinstance(token_id, int)
            and token_id != self._tokenizer.unk_token_id
            and self._tokenizer.convert_ids_to_tokens(token_id) == tag
        )

    def translate_batch(
        self,
        texts: Sequence[str],
        target_language: str,
    ) -> list[str]:
        tag = self.target_tag(target_language)
        if not self.supports_language(target_language):
            raise offline.BuildError(
                f"The MADLAD tokenizer does not contain the exact target tag {tag!r}"
            )

        # MADLAD was trained with the target-language token on the encoder
        # input.  It is not a decoder target_prefix like M2M100/NLLB.
        prompts = [f"{tag} {text}" for text in texts]
        input_ids = self._tokenizer(
            prompts,
            add_special_tokens=True,
            truncation=True,
            max_length=self._max_source_length,
        )["input_ids"]
        source_tokens = [
            self._tokenizer.convert_ids_to_tokens(ids)
            for ids in input_ids
        ]
        results = self._translator.translate_batch(
            source_tokens,
            beam_size=self._beam_size,
            max_decoding_length=self._max_decoding_length,
        )
        return [
            self._tokenizer.decode(
                self._tokenizer.convert_tokens_to_ids(result.hypotheses[0]),
                skip_special_tokens=True,
            ).strip()
            for result in results
        ]


def create_engine(args: argparse.Namespace) -> CTranslate2MadladEngine:
    if args.backend != "ctranslate2":
        raise offline.BuildError(
            "The MADLAD adapter only supports --backend ctranslate2"
        )
    return CTranslate2MadladEngine(args)


def _action(
    parser: argparse.ArgumentParser,
    destination: str,
) -> argparse.Action:
    for action in parser._actions:
        if action.dest == destination:
            return action
    raise RuntimeError(f"Parser action is missing: {destination}")


def build_argument_parser() -> argparse.ArgumentParser:
    parser = _BASE_BUILD_ARGUMENT_PARSER()
    parser.description = (
        "Generate the 13 M2M100-gap UI packs with a completely local "
        "MADLAD-400 CTranslate2 model. This command never accesses the network."
    )
    parser.set_defaults(
        backend="ctranslate2",
        compute_type="int8",
        zh_hant_mode="none",
    )
    _action(parser, "backend").choices = ("ctranslate2",)
    _action(parser, "model").help = (
        "local Nextcloud-AI/madlad400-3b-mt-ct2-int8 directory"
    )
    _action(parser, "tokenizer").help = (
        "local MADLAD tokenizer directory; defaults to --model"
    )
    _action(parser, "source_language").help = (
        "unused by MADLAD-400 (accepted for CLI compatibility)"
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    # Reuse the common, already validated pack-building pipeline while
    # replacing only its parser defaults and inference engine.
    offline.DEFAULT_LOCALE_MAP.update(MADLAD_UI_LOCALE_MAP)
    offline.build_argument_parser = build_argument_parser
    offline.create_engine = create_engine
    arguments = list(sys.argv[1:] if argv is None else argv)
    if not any(
        argument == "--locales" or argument.startswith("--locales=")
        for argument in arguments
    ):
        arguments[0:0] = [
            "--locales",
            ",".join(DEFAULT_MADLAD_LOCALES),
        ]
    return offline.main(arguments)


if __name__ == "__main__":
    raise SystemExit(main())
