import React, { useEffect, useMemo, useState } from "react";
import { Box, Scrollbar, Text, markdownToAnsi, useFrames, useMarkdownHighlighting, wrapTextWithAnsi } from "twinki";
import type { TranscriptBlock } from "../types.js";
import type { ShowcaseTheme } from "../themes.js";
const COMFORT_MESSAGES = [
  "Thinking it through...",
  "Reviewing the context...",
  "Checking the details...",
  "Taking a closer look...",
  "Still thinking...",
] as const;
interface DisplayLine {
  text: string;
  color?: string;
  background?: string;
  bold?: boolean;
  dim?: boolean;
  italic?: boolean;
  activity?: boolean;
}
function ActivityLine({ theme, startedAt }: { theme: ShowcaseTheme; startedAt: number }): React.ReactElement {
  const frame = useFrames(8);
  const elapsed = Math.floor((Date.now() - startedAt) / 1000);
  const message = COMFORT_MESSAGES[Math.min(Math.floor(elapsed / 10), COMFORT_MESSAGES.length - 1)]!;
  const text = `• ${message} (${elapsed}s • esc to interrupt)`;
  const head = (frame % (text.length + 6)) - 3,
    start = Math.max(0, Math.min(text.length, head - 1)),
    end = Math.max(start, Math.min(text.length, head + 2));
  return (
    <Text wrap="truncate">
      <Text color={theme.muted}>{text.slice(0, start)}</Text>
      <Text color={theme.accent} bold>
        {text.slice(start, end)}
      </Text>
      <Text color={theme.muted}>{text.slice(end)}</Text>
    </Text>
  );
}
function wrap(text: string, width: number): string[] {
  return wrapTextWithAnsi(text, Math.max(8, width));
}

function blockLines(block: TranscriptBlock, width: number, theme: ShowcaseTheme): DisplayLine[] {
  if (block.kind === "user") {
    return wrap(block.text, width - 2).map((text, index) => ({
      text: `${index === 0 ? "> " : "  "}${text}`,
      color: theme.fg,
      background: theme.raised,
    }));
  }
  if (block.kind === "agent") {
    const rendered = markdownToAnsi(block.text, {
      theme: theme.syntax,
      baseColor: theme.fg,
    });
    return wrap(rendered, width).map((text) => ({
      text,
      color: theme.fg,
    }));
  }
  if (block.kind === "thought") {
    const summary = block.text.replace(/\s+/g, " ").trim();
    return [
      { text: "✻ Thought", color: theme.accent, bold: true, italic: true },
      ...wrap(summary, width - 4).map((text, index, lines) => ({
        text: `  ${index === lines.length - 1 ? "╰" : "│"} ${text}`,
        color: theme.muted,
        dim: true,
        italic: true,
      })),
    ];
  }
  if (block.kind === "notice") {
    return wrap(`[!] ${block.text}`, width).map((text) => ({
      text,
      color: theme.danger,
    }));
  }
  if (!("state" in block)) return [];
  const status = block.state === "done" ? "[+]" : block.state === "error" ? "[!]" : "[~]";
  const color = block.state === "done" ? theme.success : block.state === "error" ? theme.danger : theme.warning;
  const lines: DisplayLine[] = [{ text: `${status} ${block.title}`, color, bold: true }];
  if (block.detail && block.state !== "running") {
    const details = wrap(block.detail, width - 4);
    const visible = details.slice(-3);
    for (const [index, detail] of visible.entries()) {
      lines.push({
        text: `  ${index === visible.length - 1 ? "`" : "|"} ${detail}`,
        color: theme.muted,
      });
    }
  }
  return lines;
}

export interface TranscriptProps {
  blocks: TranscriptBlock[];
  active: boolean;
  scrollFromBottom: number;
  width: number;
  height: number;
  theme: ShowcaseTheme;
  onScrollFromBottom: (value: number) => void;
}

export function Transcript({
  blocks,
  active,
  scrollFromBottom,
  width,
  height,
  theme,
  onScrollFromBottom,
}: TranscriptProps): React.ReactElement {
  const [startedAt, setStartedAt] = useState(Date.now);
  useEffect(() => {
    if (active) setStartedAt(Date.now());
  }, [active]);
  const markdown = blocks.flatMap((block) => (block.kind === "agent" ? [block.text] : [])).join("\n");
  const highlighting = useMarkdownHighlighting(markdown, theme.syntax);
  const textWidth = Math.max(10, width - 3);
  const lines = useMemo(() => {
    const output: DisplayLine[] = [];
    for (const block of blocks) {
      if (output.length > 0) output.push({ text: "" });
      output.push(...blockLines(block, textWidth, theme));
    }
    if (active && blocks.at(-1)?.kind !== "agent" && output.length > 0) output.push({ text: "" });
    if (active && blocks.at(-1)?.kind !== "agent") output.push({ text: "activity", activity: true });
    return output;
  }, [active, blocks, highlighting, textWidth, theme]);
  const viewport = Math.max(1, height);
  const maxScroll = Math.max(0, lines.length - viewport);
  const offset = Math.min(scrollFromBottom, maxScroll);
  const top = Math.max(0, lines.length - viewport - offset);
  const visible = lines.slice(top, top + viewport);

  return (
    <Box flexDirection="row" width={width} height={height} backgroundColor={theme.bg}>
      <Box flexDirection="column" width={Math.max(1, width - 1)} paddingX={1} selectionScope>
        {lines.length === 0 ? (
          <Text color={theme.muted}>ACP is ready for a prompt.</Text>
        ) : (
          Array.from({ length: viewport }, (_, index) => {
            const line = visible[index];
            if (line?.activity) return <ActivityLine key="activity" theme={theme} startedAt={startedAt} />;
            return (
              <Box
                key={`${top + index}-${line?.text ?? ""}`}
                width="100%"
                height={1}
                backgroundColor={line?.background}
              >
                <Text
                  color={line?.color ?? theme.fg}
                  backgroundColor={line?.background}
                  bold={line?.bold}
                  dimColor={line?.dim}
                  italic={line?.italic}
                  wrap="truncate"
                >
                  {line?.text ?? ""}
                </Text>
              </Box>
            );
          })
        )}
      </Box>
      <Scrollbar
        scrollTop={top}
        totalLines={lines.length}
        viewportHeight={viewport}
        color={theme.border}
        thumbColor={theme.accent}
        onScrollTo={(value) => onScrollFromBottom(maxScroll - value)}
      />
    </Box>
  );
}
