/** Format a human-readable label for a pasted image, e.g. "[pasted image (868×874 703.8 KB)]" */
export function formatImageLabel(img: {
  width?: number;
  height?: number;
  sizeBytes?: number;
}): string {
  const dims = img.width && img.height ? `${img.width}×${img.height}` : '';
  const size = img.sizeBytes
    ? img.sizeBytes >= 1024 * 1024
      ? `${(img.sizeBytes / (1024 * 1024)).toFixed(1)} MB`
      : `${(img.sizeBytes / 1024).toFixed(1)} KB`
    : '';
  const label = [dims, size].filter(Boolean).join(' ');
  return label ? `[pasted image (${label})]` : '[pasted image]';
}
