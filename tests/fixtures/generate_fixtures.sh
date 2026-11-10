#!/usr/bin/env bash
# Generates tiny 2-second stereo test audio fixtures using ffmpeg
# Left channel: 440 Hz sine, Right channel: 880 Hz sine

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

generate_fixture() {
    local filename="$1"
    shift
    ffmpeg -y \
        -f lavfi -i "sine=frequency=440:duration=2" \
        -f lavfi -i "sine=frequency=880:duration=2" \
        -filter_complex "[0:a][1:a]join=inputs=2:channel_layout=stereo[a]" \
        -map "[a]" \
        "$@" \
        "${SCRIPT_DIR}/${filename}"
}

echo "Generating audio fixtures in ${SCRIPT_DIR}..."

# 1. FLAC 16-bit
generate_fixture "flac_16bit.flac" -c:a flac -sample_fmt s16 \
    -metadata "title=Sine 440/880 16-bit" \
    -metadata "artist=Test Artist" \
    -metadata "album=Fixture Album" \
    -metadata "track=1" \
    -metadata "date=2026"

# 2. FLAC 24-bit
generate_fixture "flac_24bit.flac" -c:a flac -sample_fmt s32 -bits_per_raw_sample 24 \
    -metadata "title=Sine 440/880 24-bit" \
    -metadata "artist=Test Artist" \
    -metadata "album=Fixture Album" \
    -metadata "track=2" \
    -metadata "date=2026"

# 3. MP3
generate_fixture "mp3_test.mp3" -c:a libmp3lame -b:a 192k \
    -metadata "title=Sine MP3" \
    -metadata "artist=Test Artist" \
    -metadata "album=Fixture Album" \
    -metadata "track=3" \
    -metadata "date=2026"

# 4. M4A (AAC)
generate_fixture "m4a_aac.m4a" -c:a aac -b:a 192k \
    -metadata "title=Sine M4A AAC" \
    -metadata "artist=Test Artist" \
    -metadata "album=Fixture Album" \
    -metadata "track=4" \
    -metadata "date=2026"

# 5. WAV 16-bit
generate_fixture "wav_16bit.wav" -c:a pcm_s16le \
    -metadata "title=Sine WAV" \
    -metadata "artist=Test Artist" \
    -metadata "album=Fixture Album" \
    -metadata "track=5"

# 6. MP3 with no tags
generate_fixture "notags.mp3" -c:a libmp3lame -map_metadata -1

# 7. OGG Vorbis 44.1 kHz
generate_fixture "vorbis_44100.ogg" -c:a libvorbis -ar 44100 \
    -metadata "title=Sine Vorbis 44.1k" \
    -metadata "artist=Test Artist" \
    -metadata "album=Fixture Album" \
    -metadata "track=6" \
    -metadata "date=2026"

# 8. OGG Vorbis 48 kHz
generate_fixture "vorbis_48000.ogg" -c:a libvorbis -ar 48000 \
    -metadata "title=Sine Vorbis 48k" \
    -metadata "artist=Test Artist" \
    -metadata "album=Fixture Album" \
    -metadata "track=7" \
    -metadata "date=2026"

# 9. Truncated file (128 bytes of MP3)
head -c 128 "${SCRIPT_DIR}/mp3_test.mp3" > "${SCRIPT_DIR}/truncated.mp3"

# 8. Empty file (0 bytes)
: > "${SCRIPT_DIR}/empty.mp3"

echo "Audio fixtures generated successfully."
