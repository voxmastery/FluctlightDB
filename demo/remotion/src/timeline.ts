export type TerminalEntry = {
  kind: "command" | "prompt" | "output";
  frame: number;
  text: string;
  framesPerCharacter?: number;
};

export type VisibleTerminalEntry = {
  kind: TerminalEntry["kind"];
  text: string;
  complete: boolean;
};

export const charactersVisible = (
  frame: number,
  startFrame: number,
  framesPerCharacter: number,
  textLength: number,
): number =>
  Math.min(
    textLength,
    Math.max(0, Math.floor((frame - startFrame) / framesPerCharacter)),
  );

export const visibleTerminalEntries = (
  entries: readonly TerminalEntry[],
  frame: number,
): VisibleTerminalEntry[] =>
  entries.flatMap<VisibleTerminalEntry>((entry): VisibleTerminalEntry[] => {
    if (frame < entry.frame) {
      return [];
    }

    if (entry.kind === "output") {
      return [{kind: entry.kind, text: entry.text, complete: true}];
    }

    const visibleCharacters = charactersVisible(
      frame,
      entry.frame,
      entry.framesPerCharacter ?? 1,
      entry.text.length,
    );
    return [
      {
        kind: entry.kind,
        text: entry.text.slice(0, visibleCharacters),
        complete: visibleCharacters === entry.text.length,
      },
    ];
  });
