package io.fig.jetbrains.entries.impl;

import com.intellij.ui.content.Content;
import com.intellij.ui.content.ContentManager;
import io.fig.jetbrains.FigService;
import io.fig.jetbrains.instruments.TerminalInstrumentationType;
import io.fig.jetbrains.entries.TerminalEntry;

public class ExistingTerminalsEntry extends TerminalEntry {

    public ExistingTerminalsEntry(FigService service) {
        super(service);
    }

    @Override
    public void register(ContentManager contentManager) {
        int id = 0;

        for (Content content : contentManager.getContents())
            this.instrumentTerminalContent(content, id++, TerminalInstrumentationType.ADDED);
    }
}
