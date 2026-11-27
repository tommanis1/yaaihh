#!/usr/bin/env bash
# The tower's speech worker: a venv with the pinned recognisers, the config if none is there, the user unit.
#   ops/voice/install.sh [--parakeet]
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
venv=${YAAIHH_VOICE_VENV:-$HOME/.local/share/yaaihh-voice/venv}
conf=${YAAIHH_VOICE_CONF:-$HOME/.config/yaaihh-voice}
# the paths go into the unit by sed and into ExecStart unquoted: a space, `|`, `&`, `%` or `\` would break either
for p in "$here" "$venv" "$conf"; do
  [[ $p =~ ^[A-Za-z0-9._/+-]+$ ]] || { echo "install.sh: $p has a character the unit cannot hold; use a plain path" >&2; exit 1; }
done
python3 -c 'import sys; sys.exit(sys.version_info < (3, 11))' || { echo "install.sh: Python 3.11 or later is needed (tomllib)" >&2; exit 1; }
command -v nvidia-smi >/dev/null || { echo "install.sh: no nvidia-smi: the worker runs on the tower's GPU" >&2; exit 1; }
python3 -m venv "$venv"
# CTranslate2 4.x wants CUDA 12's cuBLAS and cuDNN 9, taken from pip rather than the system
"$venv/bin/pip" install -r "$here/requirements.txt"
[[ ${1:-} == --parakeet ]] && "$venv/bin/pip" install "nemo_toolkit[asr]==3.0.0"
site=$("$venv/bin/python" -c 'import site; print(site.getsitepackages()[0])')
mkdir -p "$conf" "$HOME/.config/systemd/user"
# the pip CUDA libraries on the loader's path, for the unit only, ahead of what the path held at install
printf 'LD_LIBRARY_PATH=%s/nvidia/cublas/lib:%s/nvidia/cudnn/lib%s\n' "$site" "$site" "${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" > "$conf/env"
[[ -e $conf/config.toml ]] || cp "$here/config.toml" "$conf/config.toml"
sed -e "/^\[Service\]/a EnvironmentFile=$conf/env" -e "s|@HERE@|$here|g; s|@VENV@|$venv|g; s|@CONF@|$conf|g" \
  "$here/yaaihh-voice.service" > "$HOME/.config/systemd/user/yaaihh-voice.service"
systemctl --user daemon-reload
echo "install.sh: edit $conf/config.toml (allow, vocab), then: systemctl --user enable --now yaaihh-voice"
