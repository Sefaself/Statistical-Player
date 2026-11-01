# Library Audit Report

- **Target Folder:** `tests/fixtures`
- **Total Audio Files Found:** 9
- **Successfully Opened & Decoded (~1s):** 7
- **Open / Probe Failures:** 2
- **Decode Failures:** 0
- **Files Missing Tags:** 4

## Codec Distribution

| Codec | Count | Share |
| :--- | :--- | :--- |
| AAC | 1 | 14.3% |
| FLAC | 2 | 28.6% |
| MP3 | 2 | 28.6% |
| PCM | 2 | 28.6% |

## Sample Rate Distribution

| Sample Rate | Count | Share |
| :--- | :--- | :--- |
| 44100 Hz | 7 | 100.0% |

## Bit Depth Distribution

| Bit Depth | Count | Share |
| :--- | :--- | :--- |
| 16-bit | 3 | 42.9% |
| 24-bit | 1 | 14.3% |
| N/A (lossy) | 3 | 42.9% |

## Files That Failed to Open / Probe

| File | Error |
| :--- | :--- |
| `tests/fixtures\empty.mp3` | Failed to probe format: unsupported feature: core (probe): no suitable format reader found |
| `tests/fixtures\truncated.mp3` | Failed to probe format: unexpected end of file |

## Files That Failed to Decode

*(None)*

## Files With Missing Tags

- `tests/fixtures\empty.mp3`
- `tests/fixtures\notags.mp3`
- `tests/fixtures\test_sine_44100.wav`
- `tests/fixtures\truncated.mp3`

