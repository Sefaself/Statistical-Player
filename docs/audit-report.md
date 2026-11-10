# Library Audit Report

- **Target Folder:** `tests/fixtures`
- **Total Audio Files Found:** 10
- **Successfully Opened & Decoded (~1s):** 8
- **Open / Probe Failures:** 2
- **Decode Failures:** 0
- **Files Missing Tags:** 3

## Codec Distribution

| Codec | Count | Share |
| :--- | :--- | :--- |
| AAC | 1 | 12.5% |
| FLAC | 2 | 25.0% |
| MP3 | 2 | 25.0% |
| PCM | 1 | 12.5% |
| Vorbis | 2 | 25.0% |

## AAC Audio Object Types

| Profile / Audio Object Type | Count | Share |
| :--- | :--- | :--- |
| LC | 1 | 100.0% |

## Sample Rate Distribution

| Sample Rate | Count | Share |
| :--- | :--- | :--- |
| 44100 Hz | 7 | 87.5% |
| 48000 Hz | 1 | 12.5% |

## Bit Depth Distribution

| Bit Depth | Count | Share |
| :--- | :--- | :--- |
| 16-bit | 2 | 25.0% |
| 24-bit | 1 | 12.5% |
| N/A (lossy) | 5 | 62.5% |

## Files That Failed to Open / Probe

| File | Error |
| :--- | :--- |
| `tests/fixtures\empty.mp3` | Failed to probe format of tests/fixtures\empty.mp3: unsupported feature: core (probe): no suitable format reader found |
| `tests/fixtures\truncated.mp3` | Failed to probe format of tests/fixtures\truncated.mp3: unexpected end of file |

## Files That Failed to Decode

*(None)*

## Files With Missing Tags

- `tests/fixtures\empty.mp3`
- `tests/fixtures\notags.mp3`
- `tests/fixtures\truncated.mp3`

