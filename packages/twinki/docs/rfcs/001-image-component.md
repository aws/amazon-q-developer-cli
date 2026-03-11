# RFC-001: Image Component

Status: Draft

## Summary

Add an `<Image>` component that renders images in the terminal using Kitty graphics protocol, iTerm2 inline images, or Sixel encoding, with a text fallback.

## Motivation

Terminal UIs increasingly need to display images — logos, charts, previews. Modern terminals support image protocols but there's no React-based abstraction for them.

## Design

### Props

```tsx
<Image
  src="./logo.png"        // path or Buffer
  width={40}              // cell width (optional, auto-scale)
  height={10}             // cell height (optional, auto-scale)
  preserveAspectRatio     // default true
  fallback="[image]"      // text when no protocol available
/>
```

### Protocol Selection

Use `detectCapabilities()` from `terminal/capabilities.ts`:
- Kitty/Ghostty/WezTerm → Kitty graphics protocol (base64 PNG, chunked)
- iTerm2 → iTerm2 inline image protocol (base64 with size hints)
- Other → text fallback

### Cell Dimensions

The TUI already queries cell size via `\x1b[16t` at startup. Use `setCellDimensions` to compute how many rows an image occupies:

```
imageRows = ceil(imageHeightPx / cellHeightPx)
```

### Image ID Management

For Kitty protocol:
- `allocateImageId()` — monotonic counter
- `deleteKittyImage(id)` — free server-side resources
- `deleteAllKittyImages()` — cleanup on unmount

### Chunked Transmission

Kitty protocol requires chunking for images > 4096 bytes:
```
\x1b_Ga=T,f=100,s=<w>,v=<h>,i=<id>,m=1;<chunk1>\x1b\\
\x1b_Gm=1;<chunk2>\x1b\\
\x1b_Gm=0;<chunkN>\x1b\\
```

## Dependencies

- Image decoding: `sharp` or native `fs.readFile` for PNG/JPEG
- Base64 encoding: Node.js `Buffer.toString('base64')`

## Open Questions

- Should Sixel be supported? Requires a Sixel encoder (complex).
- Should we support animated GIFs?
- How to handle image updates (re-render same position)?
