---
doc_meta:
  validated: 2026-08-05
  commit: 792f78393
  status: draft
  testable_headless: true
  category: setting
  title: Voice Settings
  description: Configuration options for voice input including model size, timeouts, language, and remote server
  keywords: [voice, settings, whisper, model, timeout, language, transcription, microphone, autosubmit, partial, server]
  related: [voice, voice-mode]
---

## Overview

Voice settings control how voice input behaves — the transcription model, recording timeouts, language, and remote-server fallback. All settings are optional; voice works with sensible defaults. These settings apply identically under both the V2 and KAS (V3) engines.

## Settings Reference

### voice.modelSize

Whisper model size for local transcription. Larger models are more accurate but slower and larger to download.

| Property | Value |
|----------|-------|
| Type | string |
| Default | `base` |
| Scope | global |

| Model | Download size |
|-------|---------------|
| `base` | ~148 MB |
| `small` | ~466 MB |

```bash
kiro-cli settings set voice.modelSize small
```

### voice.language

Language hint for transcription.

| Property | Value |
|----------|-------|
| Type | string |
| Default | `en` |
| Scope | global |

```bash
kiro-cli settings set voice.language es
```

### voice.silenceTimeout

Seconds of silence before recording automatically stops.

| Property | Value |
|----------|-------|
| Type | integer |
| Default | `5` |
| Scope | global |

```bash
kiro-cli settings set voice.silenceTimeout 10
```

### voice.partialPause

Pause duration in milliseconds that triggers a partial (streaming) transcription while you speak.

| Property | Value |
|----------|-------|
| Type | integer |
| Default | `500` |
| Scope | global |

```bash
kiro-cli settings set voice.partialPause 800
```

### voice.maxSessionTime

Maximum recording duration in seconds. Recording stops automatically after this time.

| Property | Value |
|----------|-------|
| Type | integer |
| Default | `300` |
| Scope | global |

```bash
kiro-cli settings set voice.maxSessionTime 600
```

### voice.autoSubmit

Whether to automatically submit transcribed text without review. When disabled, the transcription is placed in the input for you to edit before sending.

| Property | Value |
|----------|-------|
| Type | boolean |
| Default | `true` |
| Scope | global |

```bash
kiro-cli settings set voice.autoSubmit false
```

### voice.serverUrl

URL of a remote voice server for cloud desktop use. When set, the local binary is skipped and audio streams to this server instead. Also settable via the `KIRO_VOICE_SERVER_URL` environment variable.

| Property | Value |
|----------|-------|
| Type | string |
| Default | none |
| Scope | global |

```bash
kiro-cli settings set voice.serverUrl http://localhost:19876
kiro-cli settings reset voice.serverUrl
```

## Examples

### Optimize for accuracy

```bash
kiro-cli settings set voice.modelSize small
kiro-cli settings set voice.silenceTimeout 8
```

### Review before sending

```bash
kiro-cli settings set voice.autoSubmit false
```

### Cloud desktop setup

```bash
kiro-cli settings set voice.serverUrl http://localhost:19876
```

## Related

- [Voice Mode](../features/voice-mode.md) — Full feature documentation
- [/voice command](../slash-commands/voice.md) — Slash command reference
