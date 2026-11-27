"""The GPU side: the recogniser the config names (`whisper` or `parakeet`) and Silero's voice detector. Each takes
16 kHz mono int16 as an `array('h')`; numpy and the models are imported only here, so the worker's own tests need
neither."""

import re
from pathlib import Path

RATE = 16_000
COMMIT = re.compile(r"[0-9a-f]{40}")


def pinned(c, files):
    """The model's files at the config's `revision`, a full commit hash of `repo`; any other revision is refused, and
    what came is checked to be that commit's snapshot."""
    repo, rev = c.get("repo", ""), c.get("revision", "")
    if not COMMIT.fullmatch(rev):
        raise SystemExit(f"{repo or 'the model'}: revision must be a full commit hash, not {rev!r}")
    from huggingface_hub import snapshot_download

    path = Path(snapshot_download(repo, revision=rev, allow_patterns=files))
    if path.name != rev:
        raise SystemExit(f"{repo}: got {path.name}, not the pinned {rev}")
    return path


def floats(pcm):
    """int16 samples as float32 in [-1, 1], as both models and the detector take them."""
    import numpy as np

    return np.frombuffer(pcm.tobytes(), dtype=np.int16).astype(np.float32) / 32768.0


class Whisper:
    """faster-whisper (CTranslate2): large-v3-turbo, INT8 weights with FP16 compute on the 3060, told the vocab."""

    def __init__(self, c):
        from faster_whisper import WhisperModel

        self.language = c.get("language", "en")
        self.beam = int(c.get("beam_size", 5))
        self.model = WhisperModel(str(pinned(c, None)), device=c.get("device", "cuda"),
                                  device_index=int(c.get("device_index", 0)),
                                  compute_type=c.get("compute_type", "int8_float16"))

    def hear(self, pcm, prompt):
        # the detector has already cut out the silence; no context carried past the prompt the take gives
        segs, _ = self.model.transcribe(floats(pcm), language=self.language, beam_size=self.beam, initial_prompt=prompt,
                                        vad_filter=False, condition_on_previous_text=False, without_timestamps=True)
        return "".join(s.text for s in segs).strip()


class Parakeet:
    """NVIDIA Parakeet TDT 0.6B v3 through NeMo; it takes no prompt, so the vocab does not reach it."""

    def __init__(self, c):
        import nemo.collections.asr as nemo_asr

        import torch

        nemo = next(pinned(c, ["*.nemo"]).glob("*.nemo"))
        device = torch.device(c.get("device", "cuda"))
        self.model = nemo_asr.models.ASRModel.restore_from(str(nemo), map_location=device).eval()

    def hear(self, pcm, _prompt):
        out = self.model.transcribe([floats(pcm)], batch_size=1, verbose=False)
        first = out[0][0] if isinstance(out, tuple) else out[0]
        return (first.text if hasattr(first, "text") else str(first)).strip()


class Silero:
    """Silero VAD as faster-whisper carries it: where speech is in a stretch of sound, in samples."""

    def __init__(self, c):
        from faster_whisper.vad import VadOptions

        self.options = VadOptions(threshold=float(c.get("threshold", 0.5)),
                                  min_silence_duration_ms=int(c.get("min_silence_ms", 500)),
                                  speech_pad_ms=int(c.get("pad_ms", 200)))

    def spans(self, pcm):
        from faster_whisper.vad import get_speech_timestamps

        return [(t["start"], t["end"]) for t in get_speech_timestamps(floats(pcm), self.options)]


BACKENDS = {"whisper": Whisper, "parakeet": Parakeet}


def recogniser(config):
    """The recogniser config's `backend` names, loaded onto the GPU once for the worker's run."""
    name = config.get("backend", "whisper")
    if name not in BACKENDS:
        raise SystemExit(f"backend {name!r} is not one of {', '.join(BACKENDS)}")
    return BACKENDS[name](config.get(name, {}))
