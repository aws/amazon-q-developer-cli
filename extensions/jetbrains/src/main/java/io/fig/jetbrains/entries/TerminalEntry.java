package io.fig.jetbrains.entries;

import com.intellij.ui.content.Content;
import io.fig.jetbrains.FigService;
import io.fig.jetbrains.instruments.ITerminalInstrumentation;
import io.fig.jetbrains.instruments.TerminalInstrumentationType;

public abstract class TerminalEntry implements ITerminalEntry, ITerminalInstrumentation {

    private final FigService service;

    public TerminalEntry(FigService service) {
        this.service = service;
    }

    @Override
    public void instrumentTerminalContent(Content content, int id, TerminalInstrumentationType instrumentation) {
        this.service.instrumentTerminalContent(content, id, instrumentation);
    }
}
