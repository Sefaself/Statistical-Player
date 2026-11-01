# Library Audit Report

- **Target Folder:** `tests/fixtures`
- **Total Audio Files Found:** 8
- **Successfully Opened & Decoded (~1s):** 6
- **Open / Probe Failures:** 2
- **Decode Failures:** 0
- **Files Missing Tags:** 3

## Codec Distribution

| Codec | Count | Share |
| :--- | :--- | :--- |
| AAC | 1 | 16.7% |
| FLAC | 2 | 33.3% |
| MP3 | 2 | 33.3% |
| PCM | 1 | 16.7% |

## AAC Audio Object Types

| Profile / Audio Object Type | Count | Share |
| :--- | :--- | :--- |
| LC | 1 | 100.0% |

## Sample Rate Distribution

| Sample Rate | Count | Share |
| :--- | :--- | :--- |
| 44100 Hz | 6 | 100.0% |

## Bit Depth Distribution

| Bit Depth | Count | Share |
| :--- | :--- | :--- |
| 16-bit | 2 | 33.3% |
| 24-bit | 1 | 16.7% |
| N/A (lossy) | 3 | 50.0% |

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
- `tests/fixtures\truncated.mp3`

