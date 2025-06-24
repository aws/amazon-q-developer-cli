import { Code } from "@/components/text/code";
import { Kbd } from "@/components/ui/keystrokeInput";

export function parseBracketsToKbd(string: string, className?: string) {
  if (string.indexOf("[") === -1) return <span>{string}</span>;

  // This *works* but is so hacky that it feels awful and some strings have both backticks *and* square brackets...
  // TODO: switch to regex-based solution
  const strArray = string.split("[").join("*").split("]").join("*").split("*");
  const startsWith = string.charAt(0) === "[";
  const isEven = (index: number) => index % 2 === 0;

  return (
    <span>
      {startsWith
        ? strArray.map((s, i) => {
            if (isEven(i)) {
              return (
                <Kbd key={i} className={className}>
                  {s}
                </Kbd>
              );
            } else {
              return <span key={i}>{s}</span>;
            }
          })
        : strArray.map((s, i) => {
            if (!isEven(i)) {
              return (
                <Kbd key={i} className={className}>
                  {s}
                </Kbd>
              );
            } else {
              return <span key={i}>{s}</span>;
            }
          })}
    </span>
  );
}

export function parseBackticksToCode(string: string, className?: string) {
  if (string.indexOf("`") === -1) return <span>{string}</span>;

  const strArray = string.split("`");
  const startsWith = string.charAt(0) === "`";
  const isEven = (index: number) => index % 2 === 0;

  return (
    <span>
      {startsWith
        ? strArray.map((s, i) => {
            if (isEven(i)) {
              return (
                <Code key={i} className={className}>
                  {s}
                </Code>
              );
            } else {
              return <span key={i}>{s}</span>;
            }
          })
        : strArray.map((s, i) => {
            if (!isEven(i)) {
              return (
                <Code key={i} className={className}>
                  {s}
                </Code>
              );
            } else {
              return <span key={i}>{s}</span>;
            }
          })}
    </span>
  );
}
