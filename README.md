# Fortis

Real-time audio transcription in your terminal using Deepgram AI.

## Requirements

- Rust (latest stable)
- Deepgram API key ([get one here](https://deepgram.com))
- Audio input device (microphone)

## Installation

Install directly from GitHub:
```bash
cargo install --git https://github.com/nat1anwastaken/fortis.git
```

This will install the `fortis` binary to your Cargo bin directory (typically `~/.cargo/bin/`).

> **Note:** We plan to publish this to [crates.io](https://crates.io) in the future, which will allow installation via `cargo install fortis`.

## Setup

On first launch, run:
```bash
fortis
```

Press `S` to open settings and configure your Deepgram API key.

## Usage

The app will capture audio from your microphone and display transcribed text in real-time.

### Keyboard Controls

- `S` - Settings (configure API key, language, model, theme)
- `D` - Select audio input device
- `Space` - Pause/resume recording
- `Q` - Quit

## Configuration

Settings are stored in `settings.json` at your platform's config directory and can be edited via the in-app settings dialog.
