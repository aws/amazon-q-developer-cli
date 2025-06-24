package io.fig.jetbrains.entries.impl;

import com.intellij.ui.content.ContentManager;
import com.intellij.ui.content.ContentManagerEvent;
import com.intellij.ui.content.ContentManagerListener;
import io.fig.jetbrains.FigService;
import io.fig.jetbrains.instruments.TerminalInstrumentationType;
import io.fig.jetbrains.entries.TerminalEntry;
import org.jetbrains.annotations.NotNull;

public class ListenTerminalsEntry extends TerminalEntry {

    public ListenTerminalsEntry(FigService service) {
        super(service);
    }

    @Override
    public void register(ContentManager contentManager) {
        contentManager.addContentManagerListener(new ContentManagerListener() {
            @Override
            public void contentAdded(@NotNull ContentManagerEvent event) {
                ListenTerminalsEntry.this.instrumentTerminalContent(event.getContent(), event.getIndex(), TerminalInstrumentationType.ADDED);
            }

            @Override
            public void contentRemoved(@NotNull ContentManagerEvent event) {
                ListenTerminalsEntry.this.instrumentTerminalContent(event.getContent(), event.getIndex(), TerminalInstrumentationType.REMOVED);
            }
        });
    }
}
