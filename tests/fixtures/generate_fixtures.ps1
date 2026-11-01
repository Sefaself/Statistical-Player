# Generates tiny 2-second stereo test audio fixtures using ffmpeg
# Left channel: 440 Hz sine, Right channel: 880 Hz sine

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

function Generate-Fixture {
    param(
        [string]$Filename,
        [string[]]$FfmpegArgs
    )
    $OutPath = Join-Path $ScriptDir $Filename
    $BaseArgs = @(
        "-y",
        "-f", "lavfi", "-i", "sine=frequency=440:duration=2",
        "-f", "lavfi", "-i", "sine=frequency=880:duration=2",
        "-filter_complex", "[0:a][1:a]join=inputs=2:channel_layout=stereo[a]",
        "-map", "[a]"
    )
    & ffmpeg @BaseArgs @FfmpegArgs $OutPath
}

Write-Host "Generating audio fixtures in $ScriptDir..."

# 1. FLAC 16-bit
Generate-Fixture "flac_16bit.flac" @(
    "-c:a", "flac", "-sample_fmt", "s16",
    "-metadata", "title=Sine 440/880 16-bit",
    "-metadata", "artist=Test Artist",
    "-metadata", "album=Fixture Album",
    "-metadata", "track=1",
    "-metadata", "date=2026"
)

# 2. FLAC 24-bit
Generate-Fixture "flac_24bit.flac" @(
    "-c:a", "flac", "-sample_fmt", "s32", "-bits_per_raw_sample", "24",
    "-metadata", "title=Sine 440/880 24-bit",
    "-metadata", "artist=Test Artist",
    "-metadata", "album=Fixture Album",
    "-metadata", "track=2",
    "-metadata", "date=2026"
)

# 3. MP3
Generate-Fixture "mp3_test.mp3" @(
    "-c:a", "libmp3lame", "-b:a", "192k",
    "-metadata", "title=Sine MP3",
    "-metadata", "artist=Test Artist",
    "-metadata", "album=Fixture Album",
    "-metadata", "track=3",
    "-metadata", "date=2026"
)

# 4. M4A (AAC)
Generate-Fixture "m4a_aac.m4a" @(
    "-c:a", "aac", "-b:a", "192k",
    "-metadata", "title=Sine M4A AAC",
    "-metadata", "artist=Test Artist",
    "-metadata", "album=Fixture Album",
    "-metadata", "track=4",
    "-metadata", "date=2026"
)

# 5. WAV 16-bit
Generate-Fixture "wav_16bit.wav" @(
    "-c:a", "pcm_s16le",
    "-metadata", "title=Sine WAV",
    "-metadata", "artist=Test Artist",
    "-metadata", "album=Fixture Album",
    "-metadata", "track=5"
)

# 6. MP3 with no tags
Generate-Fixture "notags.mp3" @(
    "-c:a", "libmp3lame", "-map_metadata", "-1"
)

# 7. Truncated file (128 bytes of MP3)
$Mp3Path = Join-Path $ScriptDir "mp3_test.mp3"
$TruncPath = Join-Path $ScriptDir "truncated.mp3"
$Bytes = [System.IO.File]::ReadAllBytes($Mp3Path)
[System.IO.File]::WriteAllBytes($TruncPath, $Bytes[0..127])

# 8. Empty file (0 bytes)
$EmptyPath = Join-Path $ScriptDir "empty.mp3"
New-Item -ItemType File -Path $EmptyPath -Force | Out-Null

Write-Host "Audio fixtures generated successfully."
