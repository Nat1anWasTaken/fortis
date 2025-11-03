# Fortis

Real-time audio transcription in your terminal using Deepgram AI.

## Requirements

- Rust (latest stable)
- Deepgram API key ([get one here](https://deepgram.com))
- Audio input device (microphone)

## Setup

Run the application:
```bash
cargo run
```

On first launch, press `S` to open settings and configure your Deepgram API key.

## Usage

The app will capture audio from your microphone and display transcribed text in real-time.

### Keyboard Controls

- `S` - Settings (configure API key, language, model, theme)
- `D` - Select audio input device
- `Space` - Pause/resume recording
- `Q` - Quit

## Configuration

Settings are stored in `settings.json` at your platform's config directory and can be edited via the in-app settings dialog.
